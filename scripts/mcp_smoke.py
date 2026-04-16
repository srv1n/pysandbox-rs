#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import json
import os
import subprocess
import sys
import tempfile
import shutil
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_VARIANT = "ds"
EXPECTED_QUICK_STARTS = [
    "health_probe.json",
    "list_envs.json",
    "create_demo_env.json",
    "run_in_demo_env.json",
    "run_basic_stats.json",
    "run_order_summary.json",
    "run_ds_plot_synthetic_orders.json",
]


def run_checked(cmd: list[str], *, env: dict[str, str] | None = None, label: str) -> subprocess.CompletedProcess[str]:
    proc = subprocess.run(
        cmd,
        cwd=REPO_ROOT,
        env=env,
        text=True,
        capture_output=True,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"{label} failed with exit code {proc.returncode}\n"
            f"command: {' '.join(cmd)}\n"
            f"stdout:\n{proc.stdout}\n"
            f"stderr:\n{proc.stderr}"
        )
    return proc


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def quick_start_call(root: Path, filename: str) -> tuple[str, dict]:
    payload = read_json(root / filename)
    tool_name = payload["tool_id"].rsplit(":", 1)[-1]
    return tool_name, payload["args"]


def send(proc: subprocess.Popen[str], msg: dict) -> None:
    line = json.dumps(msg, separators=(",", ":"))
    assert proc.stdin is not None
    proc.stdin.write(line + "\n")
    proc.stdin.flush()


def recv(proc: subprocess.Popen[str]) -> dict:
    assert proc.stdout is not None
    decoder = json.JSONDecoder()
    while True:
        line = proc.stdout.readline()
        if not line:
            stderr = ""
            if proc.stderr is not None:
                stderr = proc.stderr.read()
            raise RuntimeError(f"worker stdout closed unexpectedly\nstderr:\n{stderr}")
        start = line.find("{")
        if start == -1:
            continue
        try:
            msg, _ = decoder.raw_decode(line[start:])
        except json.JSONDecodeError:
            continue
        return msg


class WorkerSession:
    def __init__(self, worker: Path, env: dict[str, str]) -> None:
        self.proc = subprocess.Popen(
            [str(worker)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
        )
        self.next_id = 1

    def request(self, method: str, params: dict | None = None) -> dict:
        req_id = self.next_id
        self.next_id += 1
        send(
            self.proc,
            {
                "jsonrpc": "2.0",
                "id": req_id,
                "method": method,
                "params": params or {},
            },
        )
        while True:
            msg = recv(self.proc)
            if msg.get("id") != req_id:
                continue
            if "error" in msg:
                raise RuntimeError(json.dumps(msg["error"], indent=2))
            return msg["result"]

    def call_tool(self, name: str, arguments: dict) -> dict:
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        if not isinstance(result, dict):
            raise RuntimeError(f"unexpected tool result: {result!r}")
        return result

    def close(self) -> None:
        if self.proc.poll() is not None:
            return
        try:
            send(self.proc, {"jsonrpc": "2.0", "method": "exit", "params": {}})
        except Exception:
            pass
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait(timeout=5)


def build_artifact() -> Path:
    cmd = [
        "python3",
        "scripts/build_local_release.py",
        "--variant",
        PACKAGE_VARIANT,
        "--print-artifact-path",
    ]

    def run_once() -> Path:
        proc = run_checked(cmd, label="build_local_release")
        artifact = Path(proc.stdout.strip().splitlines()[-1]).expanduser().resolve()
        if not artifact.exists():
            raise RuntimeError(f"build produced a missing artifact: {artifact}")
        return artifact

    try:
        return run_once()
    except RuntimeError as exc:
        message = str(exc)
        if "FileNotFoundError" not in message:
            raise
        shutil.rmtree(REPO_ROOT / "dist" / "install" / PACKAGE_VARIANT, ignore_errors=True)
        return run_once()


def install_artifact(artifact: Path, temp_root: Path) -> tuple[Path, Path, Path, Path]:
    install_root = temp_root / "install"
    bin_dir = temp_root / "bin"
    workflows_dir = temp_root / "workflows"
    env = os.environ.copy()
    env["RZN_PYTHON_ENVS_DIR"] = str(temp_root / "python_envs")
    env["RZN_PYTHON_TOOLS_WORKFLOWS_DIR"] = str(workflows_dir)

    run_checked(
        [
            "sh",
            "scripts/install_rzn_python_tools.sh",
            "--artifact-path",
            str(artifact),
            "--install-root",
            str(install_root),
            "--bin-dir",
            str(bin_dir),
            "--workflows-dir",
            str(workflows_dir),
        ],
        env=env,
        label="install_rzn_python_tools",
    )
    return install_root, bin_dir, workflows_dir, Path(env["RZN_PYTHON_ENVS_DIR"])


def validate_install(cli: Path, expected_variant: str, install_env: dict[str, str], workflows_dir: Path) -> tuple[dict, Path]:
    proc = run_checked([str(cli), "status", "--json"], env=install_env, label="rzn-python-tools status")
    status = json.loads(proc.stdout)

    if status["artifact"] != "rzn-python-tools":
        raise RuntimeError(f"unexpected artifact: {status['artifact']!r}")
    if status["variant"] != expected_variant:
        raise RuntimeError(f"unexpected variant: {status['variant']!r}")
    if not status["bundled_python"]:
        raise RuntimeError("expected bundled python for ds install")

    install_root = Path(status["install_root"])
    workflows_source = Path(status["workflows_source"])
    synced_quick_starts = workflows_dir / "quick_starts"
    if not install_root.exists():
        raise RuntimeError(f"install root missing: {install_root}")
    if not workflows_source.exists():
        raise RuntimeError(f"packaged workflows source missing: {workflows_source}")
    if not synced_quick_starts.exists():
        raise RuntimeError(f"synced quick-start root missing: {synced_quick_starts}")

    for filename in EXPECTED_QUICK_STARTS:
        if not (workflows_source / filename).exists():
            raise RuntimeError(f"missing packaged quick start: {workflows_source / filename}")
        if not (synced_quick_starts / filename).exists():
            raise RuntimeError(f"missing synced quick start: {synced_quick_starts / filename}")

    return status, workflows_source


def assert_tool_surface(session: WorkerSession) -> None:
    init = session.request(
        "initialize",
        {
            "protocolVersion": "2025-06-18",
            "capabilities": {"experimental": {}},
            "clientInfo": {"name": "verify", "version": "0.0.0"},
        },
    )
    server_info = init.get("serverInfo", {})
    if server_info.get("name") != "RZN Python Tools Worker":
        raise RuntimeError(f"unexpected worker name: {server_info!r}")

    tools = session.request("tools/list", {}).get("tools", [])
    names = {tool.get("name") for tool in tools}
    expected = {
        "rzn.worker.health",
        "python_env.list",
        "python_env.create",
        "python_env.install",
        "python_sandbox",
    }
    missing = expected - names
    if missing:
        raise RuntimeError(f"missing expected tools: {sorted(missing)}")


def expect_ok(result: dict, label: str) -> dict:
    if result.get("isError"):
        raise RuntimeError(f"{label} returned an error: {json.dumps(result, indent=2)}")
    return result.get("structuredContent", {})


def validate_health(session: WorkerSession) -> None:
    result = session.call_tool("rzn.worker.health", {})
    structured = expect_ok(result, "health probe")
    if structured.get("ok") is not True:
        raise RuntimeError(f"health probe did not report ok: {json.dumps(result, indent=2)}")


def validate_env_lifecycle(session: WorkerSession, quick_start_root: Path) -> None:
    initial = expect_ok(session.call_tool("python_env.list", {}), "python_env.list (initial)")
    if initial.get("envs"):
        raise RuntimeError(f"expected temp env store to start empty: {json.dumps(initial, indent=2)}")

    create_tool, create_args = quick_start_call(quick_start_root, "create_demo_env.json")
    if create_tool != "python_env.create":
        raise RuntimeError(f"unexpected create demo tool: {create_tool!r}")
    create_args = dict(create_args)
    create_args["without_pip"] = True
    created = expect_ok(
        session.call_tool(create_tool, create_args),
        "python_env.create",
    )
    if created.get("alias") != "demo" or created.get("created") is not True:
        raise RuntimeError(f"unexpected env create response: {json.dumps(created, indent=2)}")

    listed = expect_ok(session.call_tool("python_env.list", {}), "python_env.list (after create)")
    aliases = {env.get("alias") for env in listed.get("envs", [])}
    if "demo" not in aliases:
        raise RuntimeError(f"created env missing from env list: {json.dumps(listed, indent=2)}")

    run_tool, run_args = quick_start_call(quick_start_root, "run_in_demo_env.json")
    if run_tool != "python_sandbox":
        raise RuntimeError(f"unexpected run-in-env tool: {run_tool!r}")
    env_run = expect_ok(session.call_tool(run_tool, run_args), "python_sandbox managed env")
    if env_run.get("python", {}).get("kind") != "managed_env":
        raise RuntimeError(f"expected managed env runtime: {json.dumps(env_run, indent=2)}")
    output = env_run.get("output", {})
    if output.get("result", {}).get("python") is None:
        raise RuntimeError(f"managed env run did not return a python version: {json.dumps(env_run, indent=2)}")


def validate_basic_stats(session: WorkerSession, quick_start_root: Path) -> None:
    tool_name, args = quick_start_call(quick_start_root, "run_basic_stats.json")
    result = expect_ok(session.call_tool(tool_name, args), "run_basic_stats")
    output = result.get("output", {})
    expected = {"count": 6, "sum": 99, "mean": 16.5, "min": 7, "max": 25}
    if output.get("result") != expected:
        raise RuntimeError(f"unexpected basic stats output: {json.dumps(output, indent=2)}")


def validate_order_summary(session: WorkerSession, quick_start_root: Path) -> None:
    tool_name, args = quick_start_call(quick_start_root, "run_order_summary.json")
    result = expect_ok(session.call_tool(tool_name, args), "run_order_summary")
    output = result.get("output", {})
    expected = {
        "order_count": 4,
        "total_revenue": 4120.0,
        "average_order_value": 1030.0,
        "top_customer": {"name": "Acme", "revenue": 1840.0},
        "largest_order_id": "A103",
        "revenue_by_region": {"NA": 1840.0, "EU": 850.0, "APAC": 1430.0},
    }
    if output.get("result") != expected:
        raise RuntimeError(f"unexpected order summary output: {json.dumps(output, indent=2)}")


def validate_ds_chart(session: WorkerSession, quick_start_root: Path, bundled_python: Path) -> None:
    tool_name, args = quick_start_call(quick_start_root, "run_ds_plot_synthetic_orders.json")
    args = dict(args)
    args["policy_id"] = "yolo"
    args["python_path"] = str(bundled_python)
    result = expect_ok(session.call_tool(tool_name, args), "run_ds_plot_synthetic_orders")
    if result.get("python", {}).get("kind") != "explicit":
        raise RuntimeError(f"expected explicit bundled python runtime: {json.dumps(result, indent=2)}")
    output = result.get("output", {})
    payload = output.get("result", {})
    if payload.get("summary", {}).get("points") != 7:
        raise RuntimeError(f"unexpected chart summary: {json.dumps(payload, indent=2)}")
    artifacts = payload.get("artifacts", [])
    if len(artifacts) != 1 or artifacts[0].get("type") != "image":
        raise RuntimeError(f"unexpected chart artifacts: {json.dumps(payload, indent=2)}")
    png_bytes = base64.b64decode(artifacts[0].get("data_base64", ""))
    if not png_bytes.startswith(b"\x89PNG\r\n\x1a\n"):
        raise RuntimeError("chart artifact is not a PNG")


def validate_security_failure(session: WorkerSession) -> None:
    result = session.call_tool(
        "python_sandbox",
        {
            "policy_id": "yolo",
            "network_allowlist": ["localhost"],
            "inputs": {},
            "code": "import socket\nsocket.create_connection(('python.org', 443), timeout=1)",
        },
    )
    if not result.get("isError"):
        raise RuntimeError(f"expected network allowlist failure, got: {json.dumps(result, indent=2)}")
    error_text = result.get("structuredContent", {}).get("error", "")
    if "Network host not allowed" not in error_text:
        raise RuntimeError(f"unexpected security failure output: {json.dumps(result, indent=2)}")


def main() -> int:
    ap = argparse.ArgumentParser(description="Verify the packaged rzn-python-tools install over MCP.")
    ap.parse_args()

    artifact = build_artifact()

    with tempfile.TemporaryDirectory(prefix="rzn-python-tools-verify-") as tmp:
        temp_root = Path(tmp)
        install_root, bin_dir, workflows_dir, env_dir = install_artifact(artifact, temp_root)

        install_env = os.environ.copy()
        install_env["RZN_PYTHON_TOOLS_WORKFLOWS_DIR"] = str(workflows_dir)
        install_env["RZN_PYTHON_ENVS_DIR"] = str(env_dir)

        cli = bin_dir / "rzn-python-tools"
        worker = bin_dir / "rzn-python-worker"
        if not cli.exists():
            raise RuntimeError(f"installed CLI missing: {cli}")
        if not worker.exists():
            raise RuntimeError(f"installed worker wrapper missing: {worker}")

        status, quick_start_root = validate_install(cli, PACKAGE_VARIANT, install_env, workflows_dir)
        bundled_python = Path(status["install_root"]) / "resources" / "python" / "bin" / "python3"
        if not bundled_python.exists():
            raise RuntimeError(f"bundled python missing: {bundled_python}")

        worker_env = install_env.copy()
        worker_env["RZN_PLUGIN_DIR"] = str(Path(status["install_root"]))
        worker_env["RUST_LOG"] = "warn"

        session = WorkerSession(worker, worker_env)
        try:
            assert_tool_surface(session)
            validate_health(session)
            validate_env_lifecycle(session, quick_start_root)
            validate_basic_stats(session, quick_start_root)
            validate_order_summary(session, quick_start_root)
            validate_ds_chart(session, quick_start_root, bundled_python)
            validate_security_failure(session)
        finally:
            session.close()

        print(f"verified artifact: {artifact}")
        print(f"install root: {install_root}")
        print(f"workflows source: {quick_start_root}")
        print("cases: health_probe, env_lifecycle, basic_stats, order_summary, ds_chart, security_failure")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
