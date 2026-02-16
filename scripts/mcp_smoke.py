#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import sys
from pathlib import Path


def send(proc: subprocess.Popen, msg: dict) -> None:
    line = json.dumps(msg, separators=(",", ":"))
    proc.stdin.write(line + "\n")
    proc.stdin.flush()


def recv(proc: subprocess.Popen) -> dict:
    line = proc.stdout.readline()
    if not line:
        raise RuntimeError("worker stdout closed")
    return json.loads(line)


def main() -> int:
    ap = argparse.ArgumentParser(description="Smoke test the rzn-python-worker MCP stdio server.")
    ap.add_argument(
        "--worker",
        default=str(Path("target/release/rzn-python-worker")),
        help="Path to rzn-python-worker binary",
    )
    ap.add_argument(
        "--python-path",
        default=str(Path("python-bundle-minimal/bin/python3")),
        help="Explicit python executable path (used to avoid requiring an installed plugin dir)",
    )
    ap.add_argument("--policy-id", default="balanced", help="policy_id to pass to python_sandbox")
    args = ap.parse_args()

    worker = Path(args.worker).resolve()
    if not worker.exists():
        print(f"error: worker not found: {worker}", file=sys.stderr)
        return 2

    python_path = Path(args.python_path).resolve()
    if not python_path.exists():
        print(f"error: python not found: {python_path}", file=sys.stderr)
        return 2

    env = os.environ.copy()
    env.setdefault("RUST_LOG", "info")

    proc = subprocess.Popen(
        [str(worker)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
    )
    assert proc.stdin and proc.stdout

    try:
        send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {"experimental": {}},
                    "clientInfo": {"name": "smoke", "version": "0.0.0"},
                },
            },
        )
        init = recv(proc)
        print("initialize:", init.get("result", {}))

        send(proc, {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
        tools = recv(proc)
        names = [t.get("name") for t in tools.get("result", {}).get("tools", [])]
        print("tools:", names)

        send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "python_sandbox",
                    "arguments": {
                        "policy_id": args.policy_id,
                        "python_path": str(python_path),
                        "code": "print('hello from worker'); result = {'ok': True, 'n': 1+1}",
                        "inputs": {"x": 1},
                    },
                },
            },
        )
        out = recv(proc)
        print("python_sandbox:", json.dumps(out.get("result", {}), indent=2))

        # Graceful shutdown
        send(proc, {"jsonrpc": "2.0", "id": 4, "method": "shutdown", "params": {}})
    finally:
        try:
            proc.kill()
        except Exception:
            pass

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
