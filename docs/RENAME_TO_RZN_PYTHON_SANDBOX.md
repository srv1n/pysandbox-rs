# Rename Migration Notes

`pysandbox-rs` is now `rzn-python-sandbox`. Treat it like infrastructure, not a standalone branded product.

## Canonical names

| Surface | Old | New |
| --- | --- | --- |
| Repository directory | `pysandbox-rs` | `rzn-python-sandbox` |
| Cargo package | `pysandbox-rs` | `rzn-python-sandbox` |
| Rust crate import | `pysandbox` | `rzn_python_sandbox` |

## Cargo changes

Recommended dependency form:

```toml
[dependencies]
rzn_python_sandbox = { package = "rzn-python-sandbox", path = "../rzn-python-sandbox" }
```

Temporary compatibility alias if you want to delay source edits:

```toml
[dependencies]
pysandbox = { package = "rzn-python-sandbox", path = "../rzn-python-sandbox" }
```

## Source changes

Change imports from:

```rust
use pysandbox::{create_default_sandbox, ExecutionOptions};
```

to:

```rust
use rzn_python_sandbox::{create_default_sandbox, ExecutionOptions};
```

## Known downstream surfaces

| Consumer | Surface to update | Status |
| --- | --- | --- |
| `rznapp` | Cargo dependency/imports plus plugin publish/runbook docs | Queued for repo-local landing |
| `backend` | bootstrap/publish script paths that still point at the old sibling repo name | Queued for repo-local landing |
| `phone` | fallback devkit path references if the old path shim is removed | Queued for repo-local landing |

Exact queued files:

- `rznapp/src-tauri/Cargo.toml`
- `rznapp/src-tauri/src/commands/sandbox_policy.rs`
- `rznapp/src-tauri/src/commands/unified_tools.rs`
- `rznapp/docs/specs/PYTHON_SANDBOX_INTEGRATION_PLAN.md`
- `rznapp/scripts/plugins/publish_interop_rollout.py`
- `rznapp/sites/docs-dev/content/docs/developer/PYTHON_SANDBOX_ARCHITECTURE.md`
- `rznapp/docs/specs/PLUGIN_RELEASE_CHANNELS_AND_PUBLISH_CONTRACT_V1.md`
- `rznapp/docs/specs/PYTHON_SYSTEMS_WORKBENCH_UX_V1.md`
- `rznapp/docs/specs/PLUGIN_INTEROP_ROLLOUT_PUBLISH_RUNBOOK_V1.md`
- `rznapp/docs/specs/PLUGIN_PACKAGE_INTEROP_IMPLEMENTATION_V1.mapping.json`
- `backend/scripts/publish_python_tools_variants.sh`
- `backend/ops/plugin_products_seed.json`

Historical notes, progress journals, and rename planning docs in sibling repos still mention the old name intentionally. They are records, not live integration points.

## Temporary compatibility shim

For this local rollout, the old checkout path is a symlink:

```text
/Users/sarav/Downloads/side/rzn/pysandbox-rs -> /Users/sarav/Downloads/side/rzn/rzn-python-sandbox
```

That keeps existing `../pysandbox-rs` consumers from exploding immediately while the queued repo-local updates land. Do not treat the symlink as the long-term contract.

## Repo rename checklist

1. Move the local checkout to `rzn-python-sandbox`.
2. Update sibling `path = "../..."` dependencies.
3. Update any hardcoded absolute paths in release scripts.
4. If the GitHub repo is renamed, update `origin` and any docs that still point at the old URL.
