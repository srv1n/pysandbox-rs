# Python Tools Extension: Packaging + Publish Plan (RZN)

Date: 2026-02-13

This doc describes how “Python Tools” should ship as an installable RZN extension (plugin bundle) so:

- `rznapp` (desktop host) can install/update it from the backend catalog.
- Python execution can run in multiple modes (secure sandbox vs YOLO) without requiring desktop rebuilds.
- Third-party “code skills” ecosystems (ClawHub/OpenClaw) can be supported safely via developer-mode imports.

Related docs:

- Desktop strategy memo:  
  `/Users/sarav/Downloads/side/rzn/rznapp/docs/design/EXTENSIONS_BUILD_SIGN_PUBLISH_INSTALL_STRATEGY_MEMO_2026-02-13.md`
- Backend Option B spec:  
  `/Users/sarav/Downloads/side/rzn/backend/docs/specs/extensions_registry_db_and_catalog_publish_v1.md`
- Existing build spec (bundle content): `docs/PYTHON_BUNDLE_BUILD_SPEC.md`

---

## 0) What “Python Tools” Is (for users)

Python Tools is an extension that provides:

- a code runner (MCP worker) that executes Python code and returns stdout/stderr + artifacts
- a UI “Try It” runner in the desktop app (already implemented in `rznapp`)
- policy-enforced execution modes (secure vs YOLO)

The extension itself is the install/update unit.

---

## 1) Bundle Contents (what the ZIP contains)

Python Tools extension ZIP should contain:

- `plugin.json` + `plugin.sig` (signed bundle manifest)
- `rzn-python-worker` binary (MCP stdio server)
- Python runtime payload:
  - either a bundled Python distribution (recommended for determinism), or
  - a minimal interpreter + wheels/cache (future)
- Any non-code resources needed by the worker (templates, default policy config, etc)

Key point:

- If the worker is Rust/C++/Go/etc: the bundle ships a binary.
- If the worker is Python/TS/etc: the bundle ships scripts, but then the bundle must also ship the runtime OR the host must guarantee it exists.

For launch: ship the runtime with the extension (reduces “works on my machine” failures).

---

## 2) Execution Modes (Secure vs YOLO)

We treat execution modes as:

- a host policy (enterprise or user setting), and
- a worker configuration (enforcement boundary)

### 2.1 Secure mode

- deny network by default
- restrict filesystem to approved folders
- deny subprocess by default
- deterministic set of allowed packages (or no package installs)

### 2.2 YOLO mode (developer / power users)

- relaxed restrictions
- still keep guardrails (clear warnings, explicit toggles)

Important:

- The same extension bundle should support both modes.
- Mode selection should be per-run and/or per-profile, not “compile-time”.

---

## 3) Repo Layout / Ownership (keep rznapp clean)

Long-term desired state:

- `rznapp` is the host/kernel only: install/verify/spawn + UI
- Python worker source code lives outside `rznapp`

Recommendation:

- Create or move the `rzn-python-worker` crate into this repo (or a sibling repo that’s “python-tools-worker”).
- Keep `pysandbox` library here as the sandbox engine dependency for the worker.

This makes the build/publish loop independent of the desktop repo.

---

## 4) Build Outputs (what CI produces)

Each build produces artifact zips per platform (multiple variants):

```
dist/plugins/<plugin_id>/<version>/<platform>/<plugin_id>-<version>-<platform>.zip
```

Platforms:

- `macos_universal` (launch target)
- later: `windows_x86_64`, `linux_x86_64`

Variants (initial macOS launch set):

- `python-tools` (bundled minimal runtime)
- `python-tools-system` (no bundled runtime; uses system python)
- `python-tools-ds` (bundled data science runtime)

Each artifact should also produce:

- sha256 (hex) for catalog entries

---

## 5) Publish Flow (Option B, backend-owned)

This repo CI should:

1. Build the bundle artifact zip(s)
2. Upload to R2: `plugins/artifacts/python-tools/...`
3. Register release in backend DB: `POST /admin/plugins/releases`
4. (Optional) trigger publish: `POST /admin/plugins/publish`

Backend publishes:

- versioned catalog objects + pointer file (`current.json`)

Desktop consumes:

- `/plugins/index.json` (served via backend, fail-closed on signature mismatch)

---

## 6) Developer-Mode Skill Imports (ClawHub/OpenClaw compatibility)

We should assume many “skills” ecosystems will ship:

- markdown instructions
- plus Python code + dependencies

Safe integration strategy:

Stage 1 (launch):

- allow importing only `SKILL.md` as content in developer mode (no code execution)

Stage 2 (advanced):

- allow “code skills” only if:
  - user explicitly opts in, and
  - Python Tools extension is installed, and
  - execution mode is secure/YOLO with explicit warnings

Python Tools becomes the runtime boundary that makes this possible.

---

## 7) Work Breakdown (for Beads)

Suggested epic:

- “Python Tools Extension: extract worker + build/sign/publish to backend”

Stories:

- move worker crate out of `rznapp`
- build scripts in this repo produce signed zip artifacts
- CI upload + backend register
- local smoke runbook (install from file, run health/echo, run preset scripts)
