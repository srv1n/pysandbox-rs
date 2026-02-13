# Python Tools Extension (python-tools): Local Build + Install-from-file

This repo builds the `python-tools` extension ZIP (signed plugin bundle) containing:

- `rzn-python-worker` (MCP stdio sidecar)
- a bundled Python runtime under `resources/python/` (optional at runtime; the worker can also use system Python)

The fastest demo loop is **Install from file…** in the desktop host (`rznapp`).

## Prereqs

- Rust toolchain (`cargo`)
- Python 3 (`python3`) to run packager scripts
- macOS (for `macos_universal` bundle)

Optional:
- Codesigning identity (to enforce App Sandbox at the OS boundary):
  - `export RZN_MACOS_CODESIGN_IDENTITY="Developer ID Application: ..."`

## 1) Generate a dev signing keypair

```bash
cd /Users/sarav/Downloads/side/rzn/pysandbox-rs
cargo run --bin rzn-plugin-devkit -- keygen --out .secrets/plugin-signing
```

In debug builds, `rznapp` trusts `.secrets/plugin-signing/ed25519.public` automatically.

## 2) Build the worker (macos_universal)

```bash
bash scripts/build_macos_universal_worker.sh
```

Output:
- `target/universal-apple-darwin/release/rzn-python-worker`

## 3) Build the signed extension ZIP

Fast path:

```bash
bash scripts/build_python_tools_bundle_macos_universal.sh
```

Manual path:

```bash
export RZN_PYTHON_WORKER_BIN_MACOS="$(pwd)/target/universal-apple-darwin/release/rzn-python-worker"
python3 scripts/plugins/build_bundle.py \
  --config scripts/plugins/config/python-tools.json \
  --platform macos_universal \
  --key .secrets/plugin-signing/ed25519.private
```

Output:
- `dist/plugins/python-tools/0.2.0/macos_universal/python-tools-0.2.0-macos_universal.zip`

If `RZN_MACOS_CODESIGN_IDENTITY` is set, Mach-O payloads in the bundle are codesigned using:
- `entitlements/RznPythonWorker.entitlements` (worker)
- `entitlements/RznPythonInherit.entitlements` (bundled python executable)

## 4) Install from file in rznapp

In `rznapp`:

- Settings → Extensions → Install from file…
- Pick the generated ZIP.
- Enable `Python Tools`.

## 5) Smoke: run `python_sandbox` in Tool Bench

Quick health/echo shims (useful to prove the worker is wired up):

```json
{}
```

- Tool: `rzn.worker.health` → returns `ok`

```json
{ "text": "hello" }
```

- Tool: `example.echo` → returns `hello`

Call the `python_sandbox` tool with:

```json
{
  "code": "print('hello'); result = {'ok': True, 'answer': 42}",
  "policy_id": "balanced",
  "python_runtime": "auto"
}
```

Expected:
- `structuredContent.output.stdout` contains `hello`
- `structuredContent.output.result` contains the JSON result

### Artifacts (workspace output export)

In secure policies (`policy_id: enterprise|data_science|document_processing`), the worker uses the
workspace-isolated engine which exposes:
- `OUTPUT_DIR` (write files here)
- `output_files` in the structured output

When `rznapp` launches the worker it sets `RZN_APP_BASE_DIR`, so the engine additionally exports
workspace outputs into an app-controlled directory and annotates the JSON with `export_dir` and
`exported_files`.

Example:

```json
{
  "policy_id": "enterprise",
  "code": "import os\\nopen(os.path.join(OUTPUT_DIR,'hello.txt'),'w').write('hi')\\nresult={'files': os.listdir(OUTPUT_DIR)}"
}
```

Expected:
- `structuredContent.output.output_files` includes `hello.txt`
- `structuredContent.output.exported_files` includes a path under `<RZN_APP_BASE_DIR>/generated/python_exports/...`

## Runtime selection (bundled vs system)

The worker selects the interpreter in this order:

1) `python_path` tool argument (relative paths resolve against `RZN_PLUGIN_DIR` when present)
2) `RZN_PYTHON_PATH` env var (from manifest or environment)
3) `python_runtime` / `RZN_PYTHON_RUNTIME`:
   - `bundled`: require `resources/python/...`
   - `system`: use `python3` / `python` from PATH
   - `auto`: prefer bundled, else system

Bundled lookup paths:
- macOS/Linux: `resources/python/bin/python3` (fallback `.../python`)
- Windows: `resources/python/python.exe`

## Sandbox selection (policy → mode)

The host injects `policy_id` for Secure Python calls. The worker maps:

- `enterprise` / `data_science` / `document_processing` → `workspace_isolated` (uses `SandboxedPythonEngine`)
- otherwise → `native` (uses `NativePythonEngine`)

Security profiles:

- `yolo` → `SecurityProfile::Yolo` (no import restrictions)
- `balanced` → `SecurityProfile::Blacklist`
- `data_science` / `document_processing` → `SecurityProfile::DataScience`
- `enterprise` → `SecurityProfile::Strict`

You can override `execution_mode` explicitly (`native`, `workspace_isolated`, `platform_sandboxed`),
but enterprise-managed `rznapp` installs may strip/override this at the host boundary.
