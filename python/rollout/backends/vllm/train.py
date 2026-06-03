"""Phase-4 training-mode Python module for ``rollout-backend-vllm``.

Loaded by the dedicated Python OS thread under the ``train`` Cargo feature
(``crates/rollout-backend-vllm/src/train.rs::import_train_module``). Pitfall
2 + 10: the env-vars below MUST be set BEFORE ``import torch`` — the Rust
side enforces this by writing them via ``os.environ`` on the worker thread
before calling ``py.import("rollout.backends.vllm.train")``.

This module exposes a tiny synchronous surface (``init_train``,
``teardown_train``, ``forward_with_loss``, ``optimizer_step``,
``save_weights``, ``load_weights``) driven from Rust through ``py.detach``
so the GIL is released across CUDA kernels (RESEARCH Pattern 2).
"""

# =============================================================================
# DETERMINISM PREAMBLE — BEFORE import torch (Pitfall 2)
# =============================================================================
import os

os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")
os.environ.setdefault("PYTHONHASHSEED", "0")

import gc
import random
from pathlib import Path
from typing import Any, Dict, Optional

import numpy as np
import torch
from rollout.backends.vllm.qwen25_chat_template import GENERATION_MARKED_QWEN25_TEMPLATE

# Module-global Accelerator state (Pitfall 7 — Accelerator is a singleton).
_STATE: Optional[Dict[str, Any]] = None
# Pending (model_uri, seed) recorded by `configure_train`; `init_train` fires
# lazily from `_require_state` on the first forward/optimizer/save call.
_PENDING: Optional[Tuple[str, int]] = None


def configure_train(model_uri: str, seed: int = 42) -> None:
    """Record the model URI + seed for lazy ``init_train`` on first use.

    Cheap and idempotent — does NOT load the model. Rust calls this from
    ``set_train_mode(true)`` so the heavy build is deferred to the first pass.
    """
    global _PENDING
    _PENDING = (model_uri, seed)


def _require_state() -> Dict[str, Any]:
    """Return live training state, lazily building it from ``_PENDING``."""
    global _STATE
    if _STATE is None:
        if _PENDING is None:
            raise RuntimeError("init_train was not called")
        init_train(*_PENDING)
    assert _STATE is not None
    return _STATE


def _set_determinism_flags(seed: int) -> None:
    """Pitfall 2 + 8 + 10 — set every flag, in order."""
    random.seed(seed)
    np.random.seed(seed)
    torch.manual_seed(seed)
    if torch.cuda.is_available():
        torch.cuda.manual_seed_all(seed)

    torch.use_deterministic_algorithms(True, warn_only=False)
    torch.backends.cudnn.deterministic = True
    torch.backends.cudnn.benchmark = False  # Pitfall 8: MUST be False explicitly.
    torch.set_float32_matmul_precision("highest")


def init_train(model_uri: str, seed: int = 42) -> Dict[str, Any]:
    """Construct an Accelerator-wrapped model.

    Called lazily by ``forward_with_loss`` / ``optimizer_step`` / ``save_weights``
    on first use. Idempotent: returns the cached state on subsequent calls
    (Pitfall 7 — Accelerator is a singleton in a process).
    """
    global _STATE
    if _STATE is not None:
        return _STATE

    from accelerate import Accelerator
    from transformers import AutoModelForCausalLM, AutoTokenizer

    # 1. Determinism flags FIRST.
    _set_determinism_flags(seed)

    # 2. Explicit CUDA probe (Phase-3 Pitfall 9 inheritance).
    has_cuda = torch.cuda.is_available()
    device_count = torch.cuda.device_count() if has_cuda else 0

    # 3. Stateful-dataloader detection (Pitfall 3).
    try:
        import torchdata  # noqa: F401 — presence check.
        from accelerate.utils import DataLoaderConfiguration

        dl_config = DataLoaderConfiguration(use_stateful_dataloader=True)
        stateful_dataloader = True
    except ImportError:
        dl_config = None
        stateful_dataloader = False

    # 4. FSDP / DDP heuristic (DDP single-device default; FSDP if >=2 GPUs).
    accelerator_kwargs: Dict[str, Any] = {}
    if dl_config is not None:
        accelerator_kwargs["dataloader_config"] = dl_config
    if device_count >= 2:
        from accelerate.utils import FullyShardedDataParallelPlugin

        accelerator_kwargs["fsdp_plugin"] = FullyShardedDataParallelPlugin()

    accelerator = Accelerator(**accelerator_kwargs)

    # 5. Load model + tokenizer.
    tokenizer = AutoTokenizer.from_pretrained(model_uri)
    if "qwen2.5" in model_uri.lower():
        # Pitfall 1: override Qwen2.5 chat template to include generation markers.
        tokenizer.chat_template = GENERATION_MARKED_QWEN25_TEMPLATE

    model = AutoModelForCausalLM.from_pretrained(model_uri)

    # 6. Optimizer + LR scheduler.
    optimizer = torch.optim.AdamW(model.parameters(), lr=1e-5)
    scheduler = torch.optim.lr_scheduler.ConstantLR(
        optimizer, factor=1.0, total_iters=1
    )
    # Pitfall 10: keep the scheduler in the save_state capture path. If prepare
    # is incompatible with this scheduler shape, fall back to explicit register.
    try:
        model, optimizer, scheduler = accelerator.prepare(model, optimizer, scheduler)
    except Exception:  # pragma: no cover — defensive fallback.
        model, optimizer = accelerator.prepare(model, optimizer)
        accelerator.register_for_checkpointing(scheduler)

    _STATE = {
        "accelerator": accelerator,
        "model": model,
        "optimizer": optimizer,
        "scheduler": scheduler,
        "tokenizer": tokenizer,
        "has_cuda": has_cuda,
        "device_count": device_count,
        "stateful_dataloader": stateful_dataloader,
        "step": 0,
        "seed": seed,
        "last_loss": None,
    }
    return _STATE


def teardown_train() -> None:
    """Destroy training state and free CUDA memory (Pitfall 7)."""
    global _STATE, _PENDING
    _PENDING = None
    if _STATE is None:
        return
    try:
        del _STATE["model"]
        del _STATE["optimizer"]
        del _STATE["scheduler"]
        del _STATE["accelerator"]
    finally:
        _STATE = None
    gc.collect()
    if torch.cuda.is_available():
        torch.cuda.empty_cache()


def forward_with_loss(rows: list, loss_scope: str) -> Dict[str, Any]:
    """Sync; called via ``py.detach(...)`` from Rust to release the GIL.

    Returns ``{loss, n_tokens, grad_handle}``. ``loss_scope`` is one of
    ``"assistant_only"`` (Phase 4 stub; needs structured rows from plan 04-06)
    or ``"full"``.
    """
    state = _require_state()
    tokenizer = state["tokenizer"]
    model = state["model"]

    # Phase-4 placeholder packing: encode each row as a single sequence. Real
    # packing + assistant-only masking lands when SFT/RM pass structured rows
    # (plan 04-06 CLI integration + plan 04-07 examples exercise round-trip).
    enc = tokenizer(
        rows,
        padding=True,
        truncation=True,
        return_tensors="pt",
        max_length=512,
    )
    input_ids = enc["input_ids"].to(model.device)
    attn_mask = enc["attention_mask"].to(model.device)

    outputs = model(input_ids=input_ids, attention_mask=attn_mask, labels=input_ids)
    # loss_scope="assistant_only" requires re-tokenizing via apply_chat_template
    # with return_assistant_tokens_mask=True and substituting -100 elsewhere;
    # deferred to plan 04-07 smoke. The chat-template override is verified
    # independently by qwen25_assistant_mask.rs.
    _ = loss_scope  # acknowledged; unused in Phase 4.

    loss = outputs.loss
    n_tokens = int(attn_mask.sum().item())
    state["last_loss"] = loss
    next_step = state["step"] + 1
    return {
        "loss": float(loss.detach().cpu().item()),
        "n_tokens": n_tokens,
        # Opaque grad handle — Rust side reads `step` for ordering; the loss
        # tensor itself lives in module-global _STATE until optimizer_step
        # retrieves it. Phase-4 simplification (one pending grad per backend);
        # full bidirectional PyObject plumbing is a Phase-9 follow-up.
        "grad_handle": {"step": next_step},
    }


def optimizer_step(grad_handle: Dict[str, Any], lr: float) -> None:
    """Sync; called via ``py.detach(...)``. Applies deferred backward + step."""
    state = _require_state()
    loss = state.get("last_loss")
    if loss is None:
        raise RuntimeError("optimizer_step called without a pending forward_with_loss")
    optimizer = state["optimizer"]
    accelerator = state["accelerator"]

    for g in optimizer.param_groups:
        g["lr"] = lr

    accelerator.backward(loss)
    optimizer.step()
    optimizer.zero_grad()
    state["step"] = int(grad_handle["step"])
    state["last_loss"] = None


def save_weights(target_dir: str) -> str:
    """Save state to ``target_dir`` via ``accelerate.save_state``.

    Returns the path back. The Rust side tar+blake3-hashes for ``ContentId``.
    """
    state = _require_state()
    Path(target_dir).mkdir(parents=True, exist_ok=True)
    state["accelerator"].save_state(target_dir)
    return target_dir


def load_weights(src_dir: str) -> None:
    """Load state from ``src_dir`` via ``accelerate.load_state``."""
    state = _require_state()
    state["accelerator"].load_state(src_dir)
