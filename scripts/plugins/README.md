# Plugin Bundles (Python Tools): Pack + Sign

This repo can build a signed `python-tools` extension ZIP suitable for the RZN desktop host
(`rznapp`) **Install from file…** loop.

Outputs land in `dist/plugins/<id>/<version>/<platform>/` with:
- `plugin.json`
- `plugin.sig`
- `<id>-<version>-<platform>.zip`

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
