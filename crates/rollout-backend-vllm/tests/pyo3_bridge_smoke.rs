//! `pyo3-async-runtimes` GIL-release smoke test.
//!
//! Runs without `vllm` installed — uses only the stdlib `asyncio.sleep` +
//! `threading.Thread` primitives. Catches RESEARCH Pitfall 2 (GIL held across
//! the event-loop `run_until_complete` would deadlock vLLM's background tasks)
//! on every CI build:
//!
//! 1. Compile an inline Python `async def smoke()` that spawns a background
//!    Python thread which sets a module-level flag after `time.sleep(0.05)`.
//! 2. From Rust, set up a fresh asyncio event loop and drive `smoke()` through
//!    it via `pyo3_async_runtimes::tokio::run_until_complete`. The Rust future
//!    inside is `into_future(smoke())` — the same code-path the live
//!    `AsyncLLMEngine` bridge in `engine.rs::run_generate` exercises.
//! 3. Assert the round-trip completes within 2 s AND the background-thread
//!    flag was set — proving the GIL was actually released by
//!    `event_loop.run_until_complete`, not held across the await.
//!
//! The 100 ms `asyncio.sleep` is mandatory — `asyncio.sleep(0)` would
//! short-circuit and not exercise the GIL-release window.

#![cfg(feature = "vllm")]

use std::time::{Duration, Instant};

use pyo3::prelude::*;
use pyo3::types::PyModule;
use pyo3_async_runtimes::tokio::{into_future, run_until_complete};

const SMOKE_SOURCE: &str = r"
import asyncio, threading, time

_gil_released = False

def _bg():
    global _gil_released
    # If the Rust side really released the GIL during run_until_complete,
    # this Python thread runs to completion. If the GIL was held, this
    # thread blocks on the interpreter lock until run_until_complete
    # returns — but it never gets a chance to set the flag before the
    # smoke() coroutine resumes, because the GIL-release window is exactly
    # the asyncio.sleep below.
    time.sleep(0.05)
    _gil_released = True

async def smoke():
    t = threading.Thread(target=_bg)
    t.start()
    await asyncio.sleep(0.1)
    t.join(timeout=1.0)
    return _gil_released
";

#[test]
fn run_until_complete_releases_gil_across_await() {
    let start = Instant::now();
    let released: bool = Python::attach(|py| {
        let module = PyModule::from_code(
            py,
            std::ffi::CString::new(SMOKE_SOURCE).unwrap().as_c_str(),
            std::ffi::CString::new("bridge_smoke.py")
                .unwrap()
                .as_c_str(),
            std::ffi::CString::new("bridge_smoke").unwrap().as_c_str(),
        )
        .expect("compile smoke module");
        let coro = module.call_method0("smoke").expect("call smoke()");
        let coro_unbound = coro.unbind();
        let asyncio = py.import("asyncio").expect("import asyncio");
        let event_loop = asyncio
            .call_method0("new_event_loop")
            .expect("new_event_loop");
        let event_loop_for_close = event_loop.clone();
        let result: Py<PyAny> = run_until_complete::<_, Py<PyAny>>(event_loop, async move {
            let fut = Python::attach(|py| into_future(coro_unbound.into_bound(py)))?;
            fut.await
        })
        .expect("run_until_complete");
        let _ = event_loop_for_close.call_method0("close");
        result.bind(py).extract::<bool>().expect("bool return")
    });
    let elapsed = start.elapsed();

    assert!(
        released,
        "background Python thread did not run during the await — GIL was not released \
         (RESEARCH Pitfall 2 regression). The asyncio event loop's run_until_complete \
         must release the GIL while the Rust future is pending."
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "bridge took {elapsed:?} (>2s) — expected ~100ms; coroutine likely deadlocked"
    );
}

#[test]
fn into_future_round_trip_under_500ms() {
    // Round-trip overhead on a no-op coroutine should stay well under 500 ms
    // (mostly the cost of spinning up a fresh asyncio event loop). Catches
    // regressions where the bridge accidentally introduces a sleep/poll loop.
    let src = r"
import asyncio
async def noop():
    await asyncio.sleep(0)
    return 42
";
    let start = Instant::now();
    let value: i64 = Python::attach(|py| {
        let module = PyModule::from_code(
            py,
            std::ffi::CString::new(src).unwrap().as_c_str(),
            std::ffi::CString::new("noop.py").unwrap().as_c_str(),
            std::ffi::CString::new("noop_mod").unwrap().as_c_str(),
        )
        .expect("compile noop module");
        let coro_unbound = module.call_method0("noop").expect("call noop()").unbind();
        let asyncio = py.import("asyncio").expect("import asyncio");
        let event_loop = asyncio
            .call_method0("new_event_loop")
            .expect("new_event_loop");
        let event_loop_for_close = event_loop.clone();
        let result: Py<PyAny> = run_until_complete::<_, Py<PyAny>>(event_loop, async move {
            let fut = Python::attach(|py| into_future(coro_unbound.into_bound(py)))?;
            fut.await
        })
        .expect("run_until_complete noop");
        let _ = event_loop_for_close.call_method0("close");
        result.bind(py).extract::<i64>().expect("i64 return")
    });
    let elapsed = start.elapsed();

    assert_eq!(value, 42);
    assert!(
        elapsed < Duration::from_millis(500),
        "noop round-trip took {elapsed:?} (>500ms); bridge overhead too high"
    );
}
