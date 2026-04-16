# Quick Start Guide

This repo has two real entry points:

- install the packaged CLI + worker and verify the full local experience
- embed the Rust crate directly in your app

If you are unsure which one you want, start with the packaged install. It is the fastest way to answer “does this thing actually work on this machine?”

## 1. Install It and Trust It

Run this from the repo root:

```bash
make install
rzn-python-tools status
rzn-python-tools workflows sync --force
make verify
```

What this gives you:

- `make install`: installs `rzn-python-tools` and `rzn-python-worker`
- `rzn-python-tools status`: shows the installed version, variant, install root, worker path, and workflow source
- `rzn-python-tools workflows sync --force`: copies the bundled quick starts to your local workflows directory
- `make verify`: builds a fresh temp install and proves the installed worker can pass health, env lifecycle, deterministic quick starts, DS artifact output, and a negative network-allowlist case

If `make verify` passes, you have a much stronger signal than “cargo build succeeded.”

Useful follow-up commands:

```bash
rzn-python-tools paths --json
rzn-python-tools worker -- --help
```

## 2. Embed the Rust Crate

If you want the library API instead of the packaged worker, add this to `Cargo.toml`:

```toml
[dependencies]
rzn_python_sandbox = { package = "rzn-python-sandbox", path = "path/to/rzn-python-sandbox" }
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

Then start with the smallest useful example:

```rust
use rzn_python_sandbox::{create_default_sandbox, ExecutionOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sandbox = create_default_sandbox().await?;

    let result = sandbox
        .execute(
            "numbers = inputs['numbers']\nresult = {'count': len(numbers), 'sum': sum(numbers)}",
            serde_json::json!({ "numbers": [3, 5, 8] }),
            ExecutionOptions::default(),
        )
        .await?;

    println!("{result}");
    Ok(())
}
```

Expected result:

```json
{
  "count": 3,
  "sum": 16
}
```

## 3. Common Patterns

### Deterministic transform

```rust
let code = r#"
orders = inputs["orders"]
result = {
    "count": len(orders),
    "total": round(sum(order["amount"] for order in orders), 2),
}
"#;

let inputs = serde_json::json!({
    "orders": [
        { "amount": 10.5 },
        { "amount": 20.25 },
        { "amount": 4.25 }
    ]
});

let result = sandbox
    .execute(code, inputs, ExecutionOptions::default())
    .await?;
```

### Time-bounded execution

```rust
use std::time::Duration;
use rzn_python_sandbox::{ExecutionOptions, SandboxError};

let options = ExecutionOptions {
    timeout: Duration::from_secs(2),
    cpu_seconds: 1,
    memory_mb: 256,
    ..Default::default()
};

match sandbox.execute("while True: pass", serde_json::json!({}), options).await {
    Err(SandboxError::Timeout) => println!("timed out as expected"),
    other => panic!("unexpected result: {other:?}"),
}
```

### Check engine capabilities

```rust
let caps = sandbox.capabilities().await;
for cap in caps {
    println!(
        "{} | security={} | numpy={} | pandas={} | matplotlib={}",
        cap.name, cap.security_level, cap.numpy, cap.pandas, cap.matplotlib
    );
}
```

## 4. Security Reality Check

The crate does not turn Python into magic fairy dust. It gives you explicit lanes with different tradeoffs:

- `balanced`, `data_science`, and `document_processing` default to `workspace_isolated`
- `enterprise` requires `platform_sandboxed` and fails closed if the host cannot provide it
- `yolo` is intentionally the least restrictive lane for managed-env workflows

For direct crate usage, import restrictions usually surface as a `RuntimeError` carrying the Python exception text, because the worker wrapper captures Python exceptions and normalizes the result.

Example:

```rust
use rzn_python_sandbox::{ExecutionOptions, SandboxError};

match sandbox
    .execute("import os\nresult = os.getcwd()", serde_json::json!({}), ExecutionOptions::default())
    .await
{
    Err(SandboxError::RuntimeError(message)) if message.contains("blacklisted") => {
        println!("blocked as expected: {message}");
    }
    other => panic!("unexpected result: {other:?}"),
}
```

## 5. Error Handling That Matches Reality

These are the variants worth handling first:

- `SandboxError::PythonNotFound`
- `SandboxError::SyntaxError`
- `SandboxError::RuntimeError`
- `SandboxError::Timeout`
- `SandboxError::MemoryLimitExceeded`
- `SandboxError::ProcessLimitExceeded`
- `SandboxError::SecurityViolation`

Example:

```rust
use rzn_python_sandbox::SandboxError;

match sandbox.execute(code, inputs, ExecutionOptions::default()).await {
    Ok(result) => println!("success: {result}"),
    Err(SandboxError::Timeout) => eprintln!("execution timed out"),
    Err(SandboxError::MemoryLimitExceeded) => eprintln!("memory limit exceeded"),
    Err(SandboxError::SecurityViolation(msg)) => eprintln!("security violation: {msg}"),
    Err(SandboxError::RuntimeError(msg)) => eprintln!("python runtime error: {msg}"),
    Err(err) => eprintln!("sandbox error: {err}"),
}
```

## 6. Troubleshooting

### The installed CLI cannot find its files

Run:

```bash
rzn-python-tools status
rzn-python-tools paths --json
```

If those fail, your install is broken. Re-run `make install` or reinstall from an artifact.

### You need to force a Python interpreter

Use:

```bash
RZN_PYTHON_PATH=/absolute/path/to/python3 rzn-python-tools worker
```

Or, when calling the worker tool, pass `python_path`.

### You need a less opinionated runtime lane

- use the `system` variant if you trust the host machine’s Python
- use `yolo` plus managed envs if you need package installs without mutating the bundled runtime

### Enterprise mode fails on this host

That is expected when the OS-level sandbox boundary is unavailable. The repo now fails closed there on purpose.

## Next Docs

- [README.md](README.md) for the product surface and install choices
- [docs/PYTHON_TOOLS_EXTENSION_RUNBOOK.md](docs/PYTHON_TOOLS_EXTENSION_RUNBOOK.md) for install-from-file, publish, and verification flows
- [EMBEDDING_GUIDE.md](EMBEDDING_GUIDE.md) for deeper Rust integration
- [MICROSANDBOX_GUIDE.md](MICROSANDBOX_GUIDE.md) for optional VM isolation
