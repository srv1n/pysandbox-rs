# Quick Start Guide

Get up and running with pysandbox-rs in 5 minutes.

## Installation

### 1. Add to Cargo.toml

```toml
[dependencies]
pysandbox-rs = { path = "path/to/pysandbox-rs" }
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

### 2. Basic Usage

```rust
use pysandbox::{create_default_sandbox, ExecutionOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create sandbox
    let sandbox = create_default_sandbox().await?;
    
    // Execute Python code
    let code = "result = 2 + 2";
    let result = sandbox.execute(
        code,
        serde_json::json!({}),
        ExecutionOptions::default()
    ).await?;
    
    println!("Result: {}", result); // Prints: 4
    Ok(())
}
```

## Common Use Cases

### 1. Data Analysis

```rust
let code = r#"
import statistics
data = inputs['numbers']
result = {
    'mean': statistics.mean(data),
    'median': statistics.median(data),
    'stdev': statistics.stdev(data) if len(data) > 1 else 0
}
"#;

let inputs = serde_json::json!({
    "numbers": [1.5, 2.3, 3.7, 4.1, 5.9]
});

let result = sandbox.execute(code, inputs, ExecutionOptions::default()).await?;
```

### 2. JSON Processing

```rust
let code = r#"
import json
data = inputs['data']
# Transform data
result = {
    'total': sum(item['value'] for item in data),
    'count': len(data),
    'items': [item['name'] for item in data]
}
"#;

let inputs = serde_json::json!({
    "data": [
        {"name": "A", "value": 10},
        {"name": "B", "value": 20},
        {"name": "C", "value": 30}
    ]
});
```

### 3. Math Calculations

```rust
let code = r#"
import math
x = inputs['x']
result = {
    'sin': math.sin(x),
    'cos': math.cos(x),
    'tan': math.tan(x),
    'sqrt': math.sqrt(abs(x))
}
"#;

let inputs = serde_json::json!({ "x": 3.14159 });
```

## Security Features

### Import Restrictions

```rust
// This will fail - os module is blacklisted
let code = "import os; result = os.getcwd()";
match sandbox.execute(code, json!({}), ExecutionOptions::default()).await {
    Err(pysandbox::SandboxError::ImportRestriction(_)) => {
        println!("Import blocked as expected");
    }
    _ => panic!("Should have failed"),
}
```

### Resource Limits

```rust
use std::time::Duration;

let options = ExecutionOptions {
    memory_mb: 256,              // 256MB limit
    cpu_seconds: 5,              // 5 seconds CPU time
    timeout: Duration::from_secs(10), // 10 seconds wall time
    ..Default::default()
};

// This will timeout
let code = "while True: pass";
match sandbox.execute(code, json!({}), options).await {
    Err(pysandbox::SandboxError::Timeout) => {
        println!("Timed out as expected");
    }
    _ => panic!("Should have timed out"),
}
```

## Error Handling

```rust
use pysandbox::SandboxError;

match sandbox.execute(code, inputs, options).await {
    Ok(result) => {
        println!("Success: {}", result);
    }
    Err(e) => {
        match e {
            SandboxError::Timeout => {
                eprintln!("Execution timed out");
            }
            SandboxError::MemoryLimit => {
                eprintln!("Memory limit exceeded");
            }
            SandboxError::ImportRestriction(module) => {
                eprintln!("Import of '{}' is not allowed", module);
            }
            SandboxError::SyntaxError(msg) => {
                eprintln!("Syntax error: {}", msg);
            }
            SandboxError::RuntimeError(msg) => {
                eprintln!("Runtime error: {}", msg);
            }
            _ => {
                eprintln!("Error: {}", e);
            }
        }
    }
}
```

## Advanced Features

### Using Microsandbox (VM Isolation)

Enable in Cargo.toml:
```toml
pysandbox-rs = { path = "...", features = ["microsandbox-engine"] }
```

The sandbox will automatically use microsandbox if available:
```rust
let sandbox = create_default_sandbox().await?;
let caps = sandbox.capabilities().await;

for cap in caps {
    println!("{}: security level {}/10", cap.name, cap.security_level);
}
```

### Custom Import Policy

```rust
use pysandbox::{ExecutionOptions, ImportPolicy};
use std::collections::HashSet;

let mut allowed = HashSet::new();
allowed.insert("math".to_string());
allowed.insert("json".to_string());

let options = ExecutionOptions {
    import_policy: ImportPolicy::Whitelist(allowed),
    ..Default::default()
};
```

## Next Steps

- Read the [Embedding Guide](EMBEDDING_GUIDE.md) for detailed integration
- See [Tauri Integration](TAURI_INTEGRATION.md) for desktop apps
- Check [Dynamic Modules](DYNAMIC_MODULES.md) for package management
- Review [Examples](examples/) for more use cases

## Troubleshooting

### Python Not Found

Set the Python path explicitly:
```bash
export PYTHON_SYS_EXECUTABLE=/usr/bin/python3
```

### Module Not Found

Install required Python packages:
```bash
pip install numpy pandas matplotlib
```

### Permission Denied

On Unix systems, ensure the Python executable has execute permissions:
```bash
chmod +x /path/to/python3
```