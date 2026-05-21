"""Phase-4 Pitfall 1 mitigation: Qwen2.5 chat template override.

Qwen2.5 ships a chat template that does NOT include the ``{% generation %}`` /
``{% endgeneration %}`` Jinja markers. Without them, calling
``tokenizer.apply_chat_template(messages, return_assistant_tokens_mask=True)``
silently returns an all-zero (or no) mask — the SFT loss then trains the model
on every token including the prompt, which is wrong.

See HF transformers issue #34172 and rollout RESEARCH Pitfall 1. Qwen3 added the
markers natively; for Qwen2.5 we override the template at load time::

    from rollout.backends.vllm.qwen25_chat_template import (
        GENERATION_MARKED_QWEN25_TEMPLATE,
    )
    tokenizer.chat_template = GENERATION_MARKED_QWEN25_TEMPLATE
"""

GENERATION_MARKED_QWEN25_TEMPLATE = (
    "{%- for message in messages %}"
    "{%- if message.role == 'system' %}"
    "<|im_start|>system\n{{ message.content }}<|im_end|>\n"
    "{%- elif message.role == 'user' %}"
    "<|im_start|>user\n{{ message.content }}<|im_end|>\n"
    "{%- elif message.role == 'assistant' %}"
    "<|im_start|>assistant\n{% generation %}{{ message.content }}<|im_end|>{% endgeneration %}\n"
    "{%- endif %}"
    "{%- endfor %}"
)
