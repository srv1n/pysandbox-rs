# Python Tools Extension (python-tools): Local Build + Install-from-file

This repo builds signed extension ZIPs (plugin bundles) containing:

- `rzn-python-worker` (MCP stdio sidecar)
- optionally a bundled Python runtime under `resources/python/` (the worker can also use system Python)

The fastest demo loop is **Install from file…** in the desktop host (`rznapp`).

## Prereqs

- Rust toolchain (`cargo`)
- Python 3 (`python3`) to run packager scripts
- macOS (for `macos_universal` bundle)
  - Bundled runtime directories (`python-bundle-*`) are **gitignored** (generated locally).

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

Build all variations (system + minimal bundle + data science bundle):

```bash
bash scripts/build_python_tools_variants_macos_universal.sh
```

Manual path:

Build (or refresh) the bundled Python runtime if you don't already have `python-bundle-minimal/`:

```bash
bash scripts/build-python-bundle.sh --minimal --output-dir python-bundle-minimal
```

```bash
export RZN_PYTHON_WORKER_BIN_MACOS="$(pwd)/target/universal-apple-darwin/release/rzn-python-worker"
python3 scripts/plugins/build_bundle.py \
  --config scripts/plugins/config/python-tools.json \
  --platform macos_universal \
  --key .secrets/plugin-signing/ed25519.private
```

Output:
- `dist/plugins/python-tools/0.2.1/macos_universal/python-tools-0.2.1-macos_universal.zip`
- `dist/plugins/python-tools-system/0.2.1/macos_universal/python-tools-system-0.2.1-macos_universal.zip`
- `dist/plugins/python-tools-ds/0.2.1/macos_universal/python-tools-ds-0.2.1-macos_universal.zip`

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

### YOLO managed env flow (create → install → run)

Use app-managed venvs for developer workflows instead of mutating the bundled runtime.

1) Create env alias:

```json
{
  "alias": "demo"
}
```

- Tool: `python_env.create`

2) Install dependencies:

```json
{
  "alias": "demo",
  "packages": ["requests==2.32.3"]
}
```

- Tool: `python_env.install`

3) Execute with that env:

```json
{
  "policy_id": "yolo",
  "python_env": "demo",
  "code": "import requests\\nprint(requests.__version__)\\nresult={'ok': True}"
}
```

Expected:
- `structuredContent.python.kind` is `managed_env`
- `structuredContent.output.stdout` contains the dependency version

You can inspect existing envs with:
- Tool: `python_env.list`

Managed env storage root:
- `RZN_PYTHON_ENVS_DIR` (preferred explicit path)
- else `<RZN_APP_BASE_DIR>/python_envs`
- else `~/.rzn/python_envs`

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

macOS YOLO default:
- if policy is `yolo` and runtime was not explicitly set by tool args or env (`RZN_PYTHON_RUNTIME`), the worker defaults to `system`.
- this keeps dev installs compatible with pip-managed local environments while enterprise policies remain sandboxed.

Managed env override:
- `python_env` / `pythonEnv` / `env_alias` selects an app-managed virtualenv interpreter.
- for safety, managed env selection is currently allowed only with `policy_id: yolo`.

Bundled lookup paths:
- macOS/Linux: `resources/python/bin/python3` (fallback `.../python`)
- Windows: `resources/python/python.exe`

## Network allowlist (simple)

`python_sandbox` accepts optional outbound host controls:
- `network_allowlist` (or `networkAllowlist`): array of host patterns, or comma-separated string
- `RZN_PYTHON_NETWORK_ALLOWLIST`: env fallback (comma-separated)

Supported pattern forms:
- exact host: `api.openai.com`
- wildcard suffix: `*.corp.example`
- allow all: `*`

Example:

```json
{
  "policy_id": "yolo",
  "network_allowlist": ["api.openai.com", "*.github.com"],
  "code": "import urllib.request; print(urllib.request.urlopen('https://api.openai.com').status)"
}
```

In the current implementation, disallowed hosts raise `PermissionError` from a runtime socket guard.

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

## CI publish (backend Option B)

Once the backend registry/publisher is running, CI (or a local script) can publish a release:

Workflow: `.github/workflows/publish_python_tools.yml`

Required GitHub Secrets:
- `R2_PLUGINS_BUCKET`
- `R2_PLUGINS_ENDPOINT`
- `R2_PLUGINS_ACCESS_KEY_ID`
- `R2_PLUGINS_SECRET_ACCESS_KEY`
- `RZN_BACKEND_BASE_URL`
- `RZN_PLATFORM_ADMIN_TOKEN` (must be `platform_admin`)
- `RZN_PLUGIN_BUNDLE_SIGNING_PRIVATE_KEY_B64` (base64 seed; 32 bytes; 64-byte secrets accepted)

Local (optional) publish helper:

```bash
export R2_PLUGINS_BUCKET=...
export R2_PLUGINS_ENDPOINT=...
export R2_PLUGINS_ACCESS_KEY_ID=...
export R2_PLUGINS_SECRET_ACCESS_KEY=...
export RZN_BACKEND_BASE_URL="http://0.0.0.0:8082"
export RZN_PLATFORM_ADMIN_TOKEN="..."

python3 scripts/publish_python_tools_release.py --channel stable
```

Publish all variants (recommended):

```bash
python3 scripts/publish_python_tools_variants.py --channel stable
```
