"""Stdlib-only sidecar. Length-prefixed JSON over AF_UNIX.

Plan-of-record per AGENTS.md §7 + RESEARCH Pitfall 9: every plugin must be
testable locally without external services. We use 4-byte BE length-prefixed
JSON envelopes (`socket` + `struct` + `json` are all stdlib) instead of pulling
in `grpcio` / `grpclib`. Users are free to swap to gRPC in their own venv.

Wire format:
    request:  [u32 BE length][utf-8 JSON {"method": str, "payload": str}]
    response: [u32 BE length][utf-8 JSON {...}]
"""

import json
import os
import socket
import struct
import sys


def _read_exact(conn: socket.socket, n: int) -> bytes:
    buf = b""
    while len(buf) < n:
        chunk = conn.recv(n - len(buf))
        if not chunk:
            return buf
        buf += chunk
    return buf


def _handle(req: dict) -> dict:
    method = req.get("method")
    payload = req.get("payload", "")
    if method == "Init":
        return {"version": "0.1.0"}
    if method == "echo":
        return {"payload": payload}
    if method == "Shutdown":
        return {"ack": True}
    return {"error": f"unknown method: {method!r}"}


def serve(sock_path: str) -> None:
    if os.path.exists(sock_path):
        os.remove(sock_path)
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(sock_path)
    srv.listen(1)
    os.chmod(sock_path, 0o600)
    try:
        conn, _ = srv.accept()
        try:
            while True:
                hdr = _read_exact(conn, 4)
                if len(hdr) < 4:
                    break
                (n,) = struct.unpack(">I", hdr)
                body = _read_exact(conn, n)
                if len(body) < n:
                    break
                req = json.loads(body)
                resp = _handle(req)
                out = json.dumps(resp).encode()
                conn.sendall(struct.pack(">I", len(out)))
                conn.sendall(out)
                if req.get("method") == "Shutdown":
                    sys.exit(0)
        finally:
            conn.close()
    finally:
        srv.close()
        if os.path.exists(sock_path):
            os.remove(sock_path)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("usage: python -m sample_sidecar <socket_path>", file=sys.stderr)
        sys.exit(2)
    serve(sys.argv[1])
