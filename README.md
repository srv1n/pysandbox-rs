# RZN Python Sandbox

A flexible Python execution sandbox for Rust applications, positioned as RZN infrastructure for safely running model-generated code. The library provides multiple execution engines with different security and performance tradeoffs.

## Local Install

If you want a machine-local install instead of embedding the crate in Rust, use:

```bash
make install
```

That builds a release-grade local bundle, installs `rzn-python-tools` into `~/.local/bin`,
installs a wrapped `rzn-python-worker`, and syncs the packaged quick starts into
`~/.rzn/python-tools/workflows`.

The default install variant is `ds` because bundled workflows should actually run after install.
Override it if you want a smaller or system-Python setup:

```bash
make install INSTALL_VARIANT=minimal
make install INSTALL_VARIANT=system
```

Useful commands after install:

```bash
rzn-python-tools status
rzn-python-tools workflows sync --force
rzn-python-tools worker
```

`make release` builds both shell-installable local artifacts under `dist/install/` and the signed
plugin ZIPs under `dist/plugins/`.

## Library Embedding

## Features

- **Multiple Execution Engines**: 
  - Native Python with resource limits and import controls
  - Microsandbox VM-based isolation (optional)
  - Extensible trait-based architecture for future engines
  
- **Security Guardrails**:
  - Memory and CPU limits
  - Process count restrictions  
  - Import whitelist/blacklist
  - Timeout protection
  - Built-in function restrictions

- **Data Science Ready**:
  - Pre-configured for NumPy, Pandas, Matplotlib
  - Handles binary outputs (e.g., plot images)
  - JSON-based data exchange

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
rzn_python_sandbox = { package = "rzn-python-sandbox", path = "path/to/rzn-python-sandbox" }
tokio = { version = "1.42", features = ["full"] }

# Optional: Enable microsandbox support
# rzn_python_sandbox = { package = "rzn-python-sandbox", path = "...", features = ["microsandbox-engine"] }
```

Basic usage:

```rust
use rzn_python_sandbox::{create_default_sandbox, ExecutionOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create sandbox with available engines
    let sandbox = create_default_sandbox().await?;
    
    // Python code
    let code = r#"
import numpy as np
result = np.mean([1, 2, 3, 4, 5])
"#;
    
    // Execute with default options
    let result = sandbox.execute(
        code,
        serde_json::json!({}),
        ExecutionOptions::default()
    ).await?;
    
    println!("Result: {}", result);
    Ok(())
}
```

## Packaged Workflows

The repo ships a curated quick-start pack in `examples/python_sandbox/quick_starts/`.

- `run_basic_stats.json`
- `run_order_summary.json`
- `run_ds_plot_synthetic_orders.json`
- `run_ds_iris_classifier.json`
- `run_ds_usgs_quakes_chart.json`

Use `rzn-python-tools workflows sync` after install to copy the pack into your local workflows
directory, or point the desktop host at the packaged copy inside an installed plugin bundle.

## Architecture

```
┌─────────────────────┐
│   Your Application  │
└──────────┬──────────┘
           │
┌──────────▼──────────┐
│   PythonSandbox     │ (Main API)
│  ┌───────────────┐  │
│  │ PythonEngine  │  │ (Trait)
│  └───────┬───────┘  │
└──────────┼──────────┘
           │
    ┌──────┴──────┬──────────────┐
    │             │              │
┌───▼────┐  ┌────▼────┐  ┌──────▼──────┐
│Native  │  │Process  │  │Microsandbox │
│Python  │  │Isolation│  │VM Engine    │
└────────┘  └─────────┘  └─────────────┘
```

## Security Levels

1. **Native Python with Guardrails** (Level 5/10)
   - Resource limits via OS mechanisms
   - Import restrictions
   - Good for trusted/internal use

2. **Process Isolation** (Level 7/10) 
   - Subprocess with rlimit/seccomp
   - Better isolation, moderate overhead

3. **Microsandbox VM** (Level 9/10)
   - Full VM isolation
   - Best security, ~200ms overhead

## Configuration

```rust
use rzn_python_sandbox::{ExecutionOptions, ImportPolicy};
use std::time::Duration;

let options = ExecutionOptions {
    memory_mb: 1024,
    cpu_seconds: 30,
    timeout: Duration::from_secs(35),
    import_policy: ImportPolicy::data_science_whitelist(),
    env_vars: Default::default(),
};
```

## Import Policies

```rust
// Blacklist dangerous modules (default)
let policy = ImportPolicy::default();

// Whitelist only data science modules
let policy = ImportPolicy::data_science_whitelist();

// Custom whitelist
let mut whitelist = HashSet::new();
whitelist.insert("numpy".to_string());
whitelist.insert("pandas".to_string());
let policy = ImportPolicy::Whitelist(whitelist);
```

## Microsandbox Integration (Optional)

For enhanced security using VM-based isolation, you can enable microsandbox support:

```toml
[dependencies]
rzn_python_sandbox = { package = "rzn-python-sandbox", version = "0.2", features = ["microsandbox-engine"] }
```

**Note**: The library includes a custom implementation that works with microsandbox server v0.2.x using JWT authentication. When enabled and the server is running, microsandbox provides security level 9/10 through full VM isolation.

See [MICROSANDBOX_GUIDE.md](MICROSANDBOX_GUIDE.md) for setup instructions.

## Documentation

- **[Quick Start Guide](QUICKSTART.md)** - Get started in 5 minutes
- **[Python Tools Extension Runbook](docs/PYTHON_TOOLS_EXTENSION_RUNBOOK.md)** - Local install, plugin ZIPs, release artifacts, and publish flow
- **[Tauri Integration Guide](TAURI_INTEGRATION.md)** - Complete guide for embedding in Tauri applications
- **[Dynamic Modules Guide](DYNAMIC_MODULES.md)** - How to dynamically download and manage Python modules
- **[Embedding Guide](EMBEDDING_GUIDE.md)** - General guide for embedding the library in Rust applications
- **[Microsandbox Guide](MICROSANDBOX_GUIDE.md)** - Setup and configuration for VM-based isolation
- **[Rename Migration Notes](docs/RENAME_TO_RZN_PYTHON_SANDBOX.md)** - What downstream consumers need to update

## Error Handling

The library provides detailed error types:

```rust
match sandbox.execute(code, inputs, options).await {
    Ok(result) => println!("Success: {}", result),
    Err(SandboxError::Timeout) => println!("Execution timed out"),
    Err(SandboxError::MemoryLimitExceeded) => println!("Out of memory"),
    Err(SandboxError::ImportNotAllowed(module)) => {
        println!("Import {} not allowed", module)
    }
    Err(e) => println!("Error: {}", e),
}
```

## Platform Support

- **Linux**: Full support (all engines)
- **macOS**: Full support (all engines)
- **Windows**: Native engine only (microsandbox coming soon)

## Requirements

- Rust 1.70+
- Python 3.8+ (for native engine)
- Virtualization support (for microsandbox)

## License

See LICENSE file in the repository.
