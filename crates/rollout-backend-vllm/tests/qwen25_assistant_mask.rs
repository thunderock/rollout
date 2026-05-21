//! Pitfall-1 acceptance: Qwen2.5 chat-template override produces a meaningful
//! `assistant_tokens_mask`. Gated on `ROLLOUT_TRANSFORMERS_AVAILABLE=1`.
//!
//! See `python/rollout/backends/vllm/qwen25_chat_template.py` for the
//! `{% generation %}` / `{% endgeneration %}` override that makes this work.

#![cfg(feature = "train")]

use pyo3::prelude::*;
use pyo3::types::PyDict;

fn transformers_available() -> bool {
    std::env::var("ROLLOUT_TRANSFORMERS_AVAILABLE").as_deref() == Ok("1")
}

#[test]
#[ignore = "requires ROLLOUT_TRANSFORMERS_AVAILABLE=1 + transformers >= 4.45"]
fn qwen25_chat_template_assistant_mask_only_on_assistant_tokens() {
    if !transformers_available() {
        eprintln!("skipping; set ROLLOUT_TRANSFORMERS_AVAILABLE=1 to run");
        return;
    }
    Python::attach(|py| {
        // Pitfall 2: env vars BEFORE `import torch` (which train.py top-imports).
        let os = py.import("os").unwrap();
        let environ = os.getattr("environ").unwrap();
        environ.set_item("CUBLAS_WORKSPACE_CONFIG", ":4096:8").unwrap();
        environ.set_item("PYTHONHASHSEED", "0").unwrap();

        // Import the train module — runs the determinism preamble on first import.
        let train_mod = py.import("rollout.backends.vllm.train").unwrap();
        let _state = train_mod
            .call_method1("init_train", ("Qwen/Qwen2.5-0.5B-Instruct", 42_i64))
            .expect(
                "init_train failed; have you `pip install transformers accelerate torch`?",
            );

        // Pull tokenizer off module-global _STATE.
        let state = train_mod.getattr("_STATE").unwrap();
        let tokenizer = state.get_item("tokenizer").unwrap();

        // Build [user, assistant] chat.
        let user = PyDict::new(py);
        user.set_item("role", "user").unwrap();
        user.set_item("content", "Hi").unwrap();
        let assistant = PyDict::new(py);
        assistant.set_item("role", "assistant").unwrap();
        assistant.set_item("content", "Hello world").unwrap();
        let messages = vec![user, assistant];

        let kwargs = PyDict::new(py);
        kwargs.set_item("return_assistant_tokens_mask", true).unwrap();
        kwargs.set_item("return_dict", true).unwrap();
        kwargs.set_item("tokenize", true).unwrap();
        let result = tokenizer
            .call_method("apply_chat_template", (messages,), Some(&kwargs))
            .unwrap();

        let mask: Vec<i64> = result
            .get_item("assistant_tokens_mask")
            .unwrap()
            .extract()
            .unwrap();

        let ones = mask.iter().filter(|x| **x == 1).count();
        let zeros = mask.iter().filter(|x| **x == 0).count();
        assert!(ones >= 1, "mask has no assistant tokens marked: {mask:?}");
        assert!(
            zeros >= 1,
            "mask is all-ones (prompt tokens not masked out): {mask:?}"
        );
    });
}
