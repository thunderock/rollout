//! Phase-4 thread smoke test. Runs on every CI build with `--features train`
//! BUT with `transformers`/`accelerate` NOT installed — proves the dedicated
//! Python OS thread spins up and gracefully reports the import failure as a
//! typed `Fatal(PluginContract { … })` instead of panicking.
//!
//! Compile-time: also asserts `GradHandle` is `Send + Sync` (the trait dispatch
//! path moves it across thread boundaries via `oneshot::Sender`).

#![cfg(feature = "train")]

use rollout_backend_vllm::VllmBackend;
use rollout_core::TrainableBackend;

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn gradhandle_send_sync() {
    assert_send_sync::<rollout_core::GradHandle>();
}

#[test]
fn vllm_backend_is_send_sync_under_train() {
    assert_send_sync::<VllmBackend>();
}

#[tokio::test]
async fn thread_starts_under_train_feature() {
    // Construct a backend (this spawns the dedicated Python OS thread).
    let mut backend = VllmBackend::new("smoke-engine").expect("construct VllmBackend");
    // set_train_mode triggers `py.import("rollout.backends.vllm.train")` on
    // the worker thread. Without `transformers` / `accelerate` / `torch`
    // installed the import RAISES — that surfaces as
    // `Fatal(PluginContract { plugin, msg ~ "python error" })`.
    //
    // On a dev box that DOES have transformers installed the call may also
    // succeed (init_train is lazy — it just imports the module). Both
    // outcomes are accepted; the load-bearing assertion is "no panic".
    let result = backend.set_train_mode(true).await;
    match result {
        Ok(()) => {
            eprintln!("set_train_mode(true) succeeded — environment has transformers");
        }
        Err(e) => {
            let msg = format!("{e:?}");
            // Acceptable error messages — every flavour of "python deps missing".
            let ok = msg.contains("python error")
                || msg.contains("transformers")
                || msg.contains("accelerate")
                || msg.contains("ModuleNotFoundError")
                || msg.contains("torch")
                || msg.contains("No module named");
            assert!(
                ok,
                "expected python-import-failure flavoured error, got: {msg}"
            );
        }
    }
}
