#!/usr/bin/env python3
import argparse
import base64
import json
import os
import subprocess
import sys
from pathlib import Path


def send(proc: subprocess.Popen, msg: dict) -> None:
    line = json.dumps(msg, separators=(",", ":"))
    assert proc.stdin is not None
    proc.stdin.write(line + "\n")
    proc.stdin.flush()


def recv(proc: subprocess.Popen) -> dict:
    assert proc.stdout is not None
    line = proc.stdout.readline()
    if not line:
        raise RuntimeError("worker stdout closed")
    return json.loads(line)


def extract_png_bytes(result_value: object) -> bytes:
    if isinstance(result_value, dict) and result_value.get("type") == "bytes":
        data = result_value.get("data")
        if isinstance(data, str) and data.strip():
            return base64.b64decode(data)
    raise RuntimeError(f"unexpected result payload (expected bytes result): {result_value!r}")


def main() -> int:
    ap = argparse.ArgumentParser(description="Generate a chart via python_sandbox and save it to a PNG.")
    ap.add_argument(
        "--worker",
        default=str(Path("target/universal-apple-darwin/release/rzn-python-worker")),
        help="Path to rzn-python-worker binary",
    )
    ap.add_argument(
        "--python-path",
        default=str(Path("python-bundle-ds/bin/python3")),
        help="Python executable path (recommended: python-bundle-ds/bin/python3)",
    )
    ap.add_argument(
        "--out",
        default=str(Path("dist/demo_chart.png")),
        help="Output PNG path",
    )
    args = ap.parse_args()

    worker = Path(args.worker).resolve()
    python_path = Path(args.python_path).resolve()
    out_path = Path(args.out).resolve()
    out_path.parent.mkdir(parents=True, exist_ok=True)

    if not worker.exists():
        print(f"error: worker not found: {worker}", file=sys.stderr)
        return 2
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
                    "clientInfo": {"name": "demo", "version": "0.0.0"},
                },
            },
        )
        _ = recv(proc)

        code = r"""
import io
import numpy as np
import matplotlib.pyplot as plt

x = np.linspace(0, 12, 300)
y1 = np.sin(x)
y2 = np.cos(x)

fig, ax = plt.subplots(figsize=(7, 3))
ax.plot(x, y1, label="sin(x)")
ax.plot(x, y2, label="cos(x)")
ax.set_title("Python Tools demo chart")
ax.set_xlabel("x")
ax.set_ylabel("y")
ax.legend(loc="best")

buf = io.BytesIO()
fig.savefig(buf, format="png", dpi=170, bbox_inches="tight")
result = buf.getvalue()
"""

        send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "python_sandbox",
                    "arguments": {
                        "policy_id": "yolo",
                        "execution_mode": "native",
                        "python_path": str(python_path),
                        "timeout_seconds": 90,
                        "code": code,
                        "inputs": {},
                    },
                },
            },
        )
        out = recv(proc).get("result", {})
        structured = out.get("structuredContent", {})
        payload = structured.get("output", {})
        png_bytes = extract_png_bytes(payload.get("result"))

        out_path.write_bytes(png_bytes)
        print(f"wrote: {out_path}")

        send(proc, {"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": {}})
    finally:
        try:
            proc.kill()
        except Exception:
            pass

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
