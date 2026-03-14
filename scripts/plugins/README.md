# Plugin Bundles (Python Tools): Pack + Sign

This repo can build a signed `python-tools` extension ZIP suitable for the RZN desktop host
(`rznapp`) **Install from file…** loop.

Outputs land in `dist/plugins/<id>/<version>/<platform>/` with:
- `plugin.json`
- `plugin.sig`
- `<id>-<version>-<platform>.zip`

## Release Requirement

Building the ZIP is only the packaging half of the job.

If the build is intended to become visible through the backend-served plugin catalog, the release is
not done until the backend has been notified through the publish contract described in:

- `/Users/sarav/Downloads/side/rzn/backend/docs/runbook/plugin_team_release_guide.md`

That means:

- upload the artifact,
- register the release with the backend,
- publish the catalog,
- run the flow against local `http://localhost:8082` first,
- then run it against production `https://rzn.ai`.

If either publish target fails, stop there and report what failed.

## Keypair (dev)

Generate a dev signing keypair:

```bash
cargo run --bin rzn-plugin-devkit -- keygen --out .secrets/plugin-signing
```

The host (debug builds) will trust `.secrets/plugin-signing/ed25519.public` automatically.

## Build (macos_universal)

One-command build:

```bash
bash scripts/build_python_tools_bundle_macos_universal.sh
```

Manual steps (if you want more control):

1) Build a universal worker binary:

```bash
bash scripts/build_macos_universal_worker.sh
```

2) Build the plugin bundle ZIP:

```bash
export RZN_PYTHON_WORKER_BIN_MACOS="$(pwd)/target/universal-apple-darwin/release/rzn-python-worker"
python3 scripts/plugins/build_bundle.py \
  --config scripts/plugins/config/python-tools.json \
  --platform macos_universal \
  --key .secrets/plugin-signing/ed25519.private
```

## Install-from-file loop

In `rznapp`:
- Settings → Extensions → Install from file… → pick the generated ZIP
- Enable the extension, then run the `python_sandbox` tool in Tool Bench.

## Backend Publish Helpers

For local-first then production publish:

```bash
export R2_PLUGINS_BUCKET=...
export R2_PLUGINS_ENDPOINT=...
export R2_PLUGINS_ACCESS_KEY_ID=...
export R2_PLUGINS_SECRET_ACCESS_KEY=...
export RZN_PLATFORM_ADMIN_TOKEN_LOCAL=...
export RZN_PLATFORM_ADMIN_TOKEN_PROD=...

bash scripts/publish_python_tools_variants_local_and_prod.sh --channel stable
```

The helper publishes to:

- local: `http://localhost:8082`
- prod: `https://rzn.ai`

It stops on the first failure and prints which target failed.
