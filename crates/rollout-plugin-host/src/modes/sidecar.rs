//! Python sidecar loader — stdlib length-prefixed JSON framing over UDS.
//!
//! Per RESEARCH Pitfall 9 + AGENTS.md §7: the in-tree sample uses 4-byte BE
//! length-prefixed JSON over `AF_UNIX`, NOT tonic gRPC, so the cargo test
//! path never requires `pip install`.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use rollout_core::{CoreError, FatalError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};

fn internal(msg: impl Into<String>) -> CoreError {
    CoreError::Fatal(FatalError::Internal { msg: msg.into() })
}

/// Sidecar process + connected UDS stream.
pub struct SidecarState {
    /// Child process handle (None after shutdown).
    pub child: Option<Child>,
    /// Framed stream to the sidecar.
    pub stream: Option<UnixStream>,
    /// Argv used to spawn the sidecar (for respawn-on-reload).
    pub command: Vec<String>,
    /// UDS path the sidecar bound (cleaned up on unload).
    pub socket_path: PathBuf,
    /// Plugin name (for error messages).
    pub plugin_name: String,
}

impl SidecarState {
    /// Spawn the sidecar with the UDS path as argv[0+] (after the command),
    /// then connect with a 5s timeout + 50ms retry while the child binds.
    pub async fn spawn(
        command: &[String],
        socket_path: PathBuf,
        plugin_name: &str,
    ) -> Result<Self, CoreError> {
        let _ = std::fs::remove_file(&socket_path);
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| internal(format!("mkdir {}: {e}", parent.display())))?;
        }
        if command.is_empty() {
            return Err(internal("sidecar command is empty"));
        }
        let mut cmd = Command::new(&command[0]);
        cmd.args(&command[1..])
            .arg(&socket_path)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        let child = cmd
            .spawn()
            .map_err(|e| internal(format!("spawn sidecar {}: {e}", command[0])))?;

        let stream = connect_with_retry(&socket_path, Duration::from_secs(5)).await?;
        Ok(Self {
            child: Some(child),
            stream: Some(stream),
            command: command.to_vec(),
            socket_path,
            plugin_name: plugin_name.to_owned(),
        })
    }

    /// Send a `{ "method": method, "payload": <utf8 payload> }` envelope and
    /// read a length-prefixed JSON response. Returns the response body bytes.
    pub async fn call(&mut self, method: &str, payload: &[u8]) -> Result<Vec<u8>, CoreError> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| internal("sidecar stream not connected"))?;
        // Encode payload as a JSON-safe string (UTF-8 lossy) — sidecars that
        // need binary payloads should base64-encode; the in-tree sample is
        // text-only.
        let req = serde_json::json!({
            "method": method,
            "payload": String::from_utf8_lossy(payload),
        });
        let body = serde_json::to_vec(&req)
            .map_err(|e| internal(format!("sidecar request encode: {e}")))?;
        let len = u32::try_from(body.len()).map_err(|_| internal("sidecar request too large"))?;
        stream
            .write_all(&len.to_be_bytes())
            .await
            .map_err(|e| internal(format!("sidecar write len: {e}")))?;
        stream
            .write_all(&body)
            .await
            .map_err(|e| internal(format!("sidecar write body: {e}")))?;
        stream
            .flush()
            .await
            .map_err(|e| internal(format!("sidecar flush: {e}")))?;

        let mut hdr = [0u8; 4];
        stream
            .read_exact(&mut hdr)
            .await
            .map_err(|e| internal(format!("sidecar read len: {e}")))?;
        let n = u32::from_be_bytes(hdr) as usize;
        let mut buf = vec![0u8; n];
        stream
            .read_exact(&mut buf)
            .await
            .map_err(|e| internal(format!("sidecar read body: {e}")))?;
        Ok(buf)
    }

    /// Best-effort graceful shutdown: send `{ "method": "Shutdown" }`, then
    /// reap the child within 2s; SIGKILL if it lingers.
    pub async fn shutdown(&mut self) -> Result<(), CoreError> {
        if let Some(stream) = self.stream.as_mut() {
            let body = serde_json::to_vec(&serde_json::json!({"method": "Shutdown"}))
                .map_err(|e| internal(format!("sidecar shutdown encode: {e}")))?;
            #[allow(clippy::cast_possible_truncation)]
            let len = body.len() as u32;
            let _ = stream.write_all(&len.to_be_bytes()).await;
            let _ = stream.write_all(&body).await;
            let _ = stream.flush().await;
        }
        if let Some(mut child) = self.child.take() {
            let waited = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
            if waited.is_err() {
                let _ = child.kill().await;
            }
        }
        self.stream = None;
        let _ = std::fs::remove_file(&self.socket_path);
        Ok(())
    }

    /// SIGTERM the child, wait up to 2s, SIGKILL if needed, then respawn with
    /// the same command line on a fresh UDS path.
    #[cfg(feature = "dev-hot-reload")]
    pub async fn respawn(&mut self) -> Result<(), CoreError> {
        if let Some(child) = self.child.as_mut() {
            if let Some(pid) = child.id() {
                #[cfg(feature = "sidecar")]
                {
                    // pid u32 → i32 wrap-safe in practice (PIDs are positive
                    // and well below i32::MAX on Linux/macOS).
                    let pid_i32 = i32::try_from(pid).unwrap_or(i32::MAX);
                    let raw = nix::unistd::Pid::from_raw(pid_i32);
                    let _ = nix::sys::signal::kill(raw, nix::sys::signal::Signal::SIGTERM);
                }
            }
            let waited = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
            if waited.is_err() {
                let _ = child.kill().await;
            }
        }
        self.child = None;
        self.stream = None;
        let _ = std::fs::remove_file(&self.socket_path);
        let new_path = self.socket_path.clone();
        let fresh = Self::spawn(&self.command.clone(), new_path, &self.plugin_name).await?;
        self.child = fresh.child;
        self.stream = fresh.stream;
        self.socket_path = fresh.socket_path;
        Ok(())
    }
}

async fn connect_with_retry(
    path: &std::path::Path,
    timeout: Duration,
) -> Result<UnixStream, CoreError> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match UnixStream::connect(path).await {
            Ok(s) => return Ok(s),
            Err(_) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => return Err(internal(format!("connect {}: {e}", path.display()))),
        }
    }
}
