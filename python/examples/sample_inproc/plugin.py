"""In-process PyO3 plugin sample. Stdlib only.

Contract per docs/specs/03-plugin-system.md §3.2: the factory returns an
object with a `call(method: str, payload: bytes) -> bytes` method.
"""


class _Plugin:
    def call(self, method: str, payload: bytes) -> bytes:
        if method == "echo":
            return payload
        if method == "ping":
            return b"pong"
        raise ValueError(f"unknown method: {method!r}")


def create_plugin():
    return _Plugin()
