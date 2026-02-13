# Embedding Guide

This guide explains how to embed pysandbox-rs in your Rust application as a library.

## Table of Contents
1. [Library Setup](#library-setup)
2. [Basic Integration](#basic-integration)
3. [Advanced Configuration](#advanced-configuration)
4. [Error Handling](#error-handling)
5. [Performance Optimization](#performance-optimization)
6. [Platform-Specific Notes](#platform-specific-notes)

## Library Setup

### Adding as a Dependency

There are several ways to include pysandbox-rs in your project:

#### 1. Local Path
```toml
[dependencies]
pysandbox-rs = { path = "../pysandbox-rs" }
```

#### 2. Git Repository
```toml
[dependencies]
pysandbox-rs = { git = "https://github.com/yourusername/pysandbox-rs" }
```

#### 3. With Optional Features
```toml
[dependencies]
pysandbox-rs = { 
    path = "../pysandbox-rs",
    features = ["microsandbox-engine"]
}
```

### Workspace Setup

For larger projects, use a workspace:

```toml
# Workspace Cargo.toml
[workspace]
members = [
    "app",
    "pysandbox-rs"
]

[workspace.dependencies]
tokio = { version = "1.42", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
```

## Basic Integration

### Minimal Example

```rust
use pysandbox::{create_default_sandbox, ExecutionOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create sandbox
    let sandbox = create_default_sandbox().await?;
    
    // Execute Python code
    let code = r#"
result = sum([1, 2, 3, 4, 5])
"#;
    
    let result = sandbox.execute(
        code,
        serde_json::json!({}),
        ExecutionOptions::default()
    ).await?;
    
    println!("Result: {}", result);
    Ok(())
}
```

### With Input Data

```rust
use pysandbox::{create_default_sandbox, ExecutionOptions};
use serde_json::json;

async fn process_data(numbers: Vec<f64>) -> Result<f64, Box<dyn std::error::Error>> {
    let sandbox = create_default_sandbox().await?;
    
    let code = r#"
import statistics
result = statistics.mean(inputs['numbers'])
"#;
    
    let inputs = json!({
        "numbers": numbers
    });
    
    let result = sandbox.execute(code, inputs, ExecutionOptions::default()).await?;
    
    Ok(result.as_f64().unwrap_or(0.0))
}
```

### Structured Output

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct AnalysisResult {
    mean: f64,
    median: f64,
    std_dev: f64,
    min: f64,
    max: f64,
}

async fn analyze_data(data: Vec<f64>) -> Result<AnalysisResult, Box<dyn std::error::Error>> {
    let sandbox = create_default_sandbox().await?;
    
    let code = r#"
import statistics
import json

data = inputs['data']
result = {
    'mean': statistics.mean(data),
    'median': statistics.median(data),
    'std_dev': statistics.stdev(data) if len(data) > 1 else 0,
    'min': min(data),
    'max': max(data)
}
"#;
    
    let result = sandbox.execute(
        code,
        json!({ "data": data }),
        ExecutionOptions::default()
    ).await?;
    
    let analysis: AnalysisResult = serde_json::from_value(result)?;
    Ok(analysis)
}
```

## Advanced Configuration

### Custom Execution Options

```rust
use pysandbox::{ExecutionOptions, ImportPolicy};
use std::time::Duration;
use std::collections::{HashMap, HashSet};

fn create_custom_options() -> ExecutionOptions {
    let mut allowed_modules = HashSet::new();
    allowed_modules.insert("numpy".to_string());
    allowed_modules.insert("pandas".to_string());
    allowed_modules.insert("json".to_string());
    allowed_modules.insert("math".to_string());
    
    ExecutionOptions {
        memory_mb: 1024,           // 1GB memory limit
        cpu_seconds: 60,           // 60 seconds CPU time
        timeout: Duration::from_secs(65), // 65 seconds wall time
        import_policy: ImportPolicy::Whitelist(allowed_modules),
        env_vars: HashMap::new(),
    }
}
```

### Sandbox Manager

Create a manager for handling multiple sandboxes:

```rust
use pysandbox::{PythonSandbox, create_default_sandbox, ExecutionOptions};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use std::collections::HashMap;

pub struct SandboxManager {
    sandboxes: Arc<RwLock<HashMap<String, Arc<Mutex<PythonSandbox>>>>>,
    max_sandboxes: usize,
}

impl SandboxManager {
    pub fn new(max_sandboxes: usize) -> Self {
        Self {
            sandboxes: Arc::new(RwLock::new(HashMap::new())),
            max_sandboxes,
        }
    }
    
    pub async fn get_or_create(&self, id: &str) -> Result<Arc<Mutex<PythonSandbox>>, String> {
        // Check if sandbox exists
        {
            let sandboxes = self.sandboxes.read().await;
            if let Some(sandbox) = sandboxes.get(id) {
                return Ok(Arc::clone(sandbox));
            }
        }
        
        // Create new sandbox
        let mut sandboxes = self.sandboxes.write().await;
        
        // Check limit
        if sandboxes.len() >= self.max_sandboxes {
            return Err("Maximum number of sandboxes reached".to_string());
        }
        
        // Create sandbox
        let sandbox = create_default_sandbox().await
            .map_err(|e| format!("Failed to create sandbox: {}", e))?;
        
        let sandbox = Arc::new(Mutex::new(sandbox));
        sandboxes.insert(id.to_string(), Arc::clone(&sandbox));
        
        Ok(sandbox)
    }
    
    pub async fn remove(&self, id: &str) -> Option<Arc<Mutex<PythonSandbox>>> {
        let mut sandboxes = self.sandboxes.write().await;
        sandboxes.remove(id)
    }
    
    pub async fn clear(&self) {
        let mut sandboxes = self.sandboxes.write().await;
        sandboxes.clear();
    }
}
```

### Custom Engine Implementation

Implement your own execution engine:

```rust
use async_trait::async_trait;
use pysandbox::{
    engine::{PythonEngine, ExecutionOptions, EngineCapabilities},
    errors::{Result, SandboxError},
};

pub struct CustomEngine {
    // Your custom fields
}

#[async_trait]
impl PythonEngine for CustomEngine {
    async fn validate(&self, code: &str, options: &ExecutionOptions) -> Result<()> {
        // Custom validation logic
        if code.contains("dangerous_function") {
            return Err(SandboxError::SecurityViolation(
                "Dangerous function detected".to_string()
            ));
        }
        Ok(())
    }
    
    async fn execute(
        &mut self,
        code: &str,
        inputs: serde_json::Value,
        options: &ExecutionOptions,
    ) -> Result<serde_json::Value> {
        // Custom execution logic
        todo!()
    }
    
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: "Custom Engine".to_string(),
            numpy: false,
            matplotlib: false,
            pandas: false,
            max_memory_mb: 512,
            max_cpu_seconds: 30,
            security_level: 7,
        }
    }
    
    async fn shutdown(&mut self) -> Result<()> {
        // Cleanup logic
        Ok(())
    }
}
```

## Error Handling

### Comprehensive Error Handling

```rust
use pysandbox::{SandboxError, create_default_sandbox, ExecutionOptions};

async fn safe_execute(code: &str) -> Result<String, String> {
    let sandbox = match create_default_sandbox().await {
        Ok(s) => s,
        Err(e) => return Err(format!("Failed to create sandbox: {}", e)),
    };
    
    match sandbox.execute(code, serde_json::json!({}), ExecutionOptions::default()).await {
        Ok(result) => Ok(result.to_string()),
        Err(e) => {
            match e {
                SandboxError::Timeout => Err("Code execution timed out".to_string()),
                SandboxError::MemoryLimit => Err("Memory limit exceeded".to_string()),
                SandboxError::ImportRestriction(module) => {
                    Err(format!("Import of '{}' is not allowed", module))
                },
                SandboxError::SecurityViolation(msg) => {
                    Err(format!("Security violation: {}", msg))
                },
                SandboxError::SyntaxError(msg) => {
                    Err(format!("Python syntax error: {}", msg))
                },
                SandboxError::RuntimeError(msg) => {
                    Err(format!("Runtime error: {}", msg))
                },
                _ => Err(format!("Execution failed: {}", e)),
            }
        }
    }
}
```

### Retry Logic

```rust
use tokio::time::{sleep, Duration};

async fn execute_with_retry(
    code: &str,
    max_retries: u32,
) -> Result<serde_json::Value, String> {
    let mut attempts = 0;
    let mut last_error = String::new();
    
    while attempts < max_retries {
        match create_default_sandbox().await {
            Ok(sandbox) => {
                match sandbox.execute(code, serde_json::json!({}), ExecutionOptions::default()).await {
                    Ok(result) => return Ok(result),
                    Err(e) => {
                        last_error = e.to_string();
                        if matches!(e, SandboxError::Timeout | SandboxError::InternalError(_)) {
                            // Retry on transient errors
                            attempts += 1;
                            sleep(Duration::from_millis(100 * attempts as u64)).await;
                            continue;
                        } else {
                            // Don't retry on permanent errors
                            return Err(e.to_string());
                        }
                    }
                }
            }
            Err(e) => {
                last_error = e.to_string();
                attempts += 1;
                sleep(Duration::from_millis(100 * attempts as u64)).await;
            }
        }
    }
    
    Err(format!("Failed after {} attempts: {}", max_retries, last_error))
}
```

## Performance Optimization

### Sandbox Pooling

```rust
use tokio::sync::mpsc;
use std::sync::Arc;

pub struct SandboxPool {
    sender: mpsc::Sender<PythonSandbox>,
    receiver: Arc<Mutex<mpsc::Receiver<PythonSandbox>>>,
}

impl SandboxPool {
    pub async fn new(size: usize) -> Result<Self, Box<dyn std::error::Error>> {
        let (tx, rx) = mpsc::channel(size);
        
        // Pre-create sandboxes
        for _ in 0..size {
            let sandbox = create_default_sandbox().await?;
            tx.send(sandbox).await?;
        }
        
        Ok(Self {
            sender: tx,
            receiver: Arc::new(Mutex::new(rx)),
        })
    }
    
    pub async fn acquire(&self) -> Option<SandboxGuard> {
        let mut receiver = self.receiver.lock().await;
        receiver.recv().await.map(|sandbox| {
            SandboxGuard {
                sandbox: Some(sandbox),
                sender: self.sender.clone(),
            }
        })
    }
}

pub struct SandboxGuard {
    sandbox: Option<PythonSandbox>,
    sender: mpsc::Sender<PythonSandbox>,
}

impl SandboxGuard {
    pub async fn execute(
        &self,
        code: &str,
        inputs: serde_json::Value,
        options: ExecutionOptions,
    ) -> Result<serde_json::Value, SandboxError> {
        self.sandbox.as_ref().unwrap().execute(code, inputs, options).await
    }
}

impl Drop for SandboxGuard {
    fn drop(&mut self) {
        if let Some(sandbox) = self.sandbox.take() {
            // Return sandbox to pool
            let sender = self.sender.clone();
            tokio::spawn(async move {
                let _ = sender.send(sandbox).await;
            });
        }
    }
}
```

### Caching Results

```rust
use lru::LruCache;
use std::num::NonZeroUsize;
use sha2::{Sha256, Digest};

pub struct CachedSandbox {
    sandbox: PythonSandbox,
    cache: Arc<Mutex<LruCache<String, serde_json::Value>>>,
}

impl CachedSandbox {
    pub async fn new(cache_size: usize) -> Result<Self, Box<dyn std::error::Error>> {
        let sandbox = create_default_sandbox().await?;
        let cache = LruCache::new(NonZeroUsize::new(cache_size).unwrap());
        
        Ok(Self {
            sandbox,
            cache: Arc::new(Mutex::new(cache)),
        })
    }
    
    pub async fn execute_cached(
        &self,
        code: &str,
        inputs: serde_json::Value,
        options: ExecutionOptions,
    ) -> Result<serde_json::Value, SandboxError> {
        // Generate cache key
        let mut hasher = Sha256::new();
        hasher.update(code.as_bytes());
        hasher.update(serde_json::to_string(&inputs).unwrap().as_bytes());
        let cache_key = format!("{:x}", hasher.finalize());
        
        // Check cache
        {
            let mut cache = self.cache.lock().await;
            if let Some(result) = cache.get(&cache_key) {
                return Ok(result.clone());
            }
        }
        
        // Execute code
        let result = self.sandbox.execute(code, inputs, options).await?;
        
        // Store in cache
        {
            let mut cache = self.cache.lock().await;
            cache.put(cache_key, result.clone());
        }
        
        Ok(result)
    }
}
```

## Platform-Specific Notes

### Windows

```rust
#[cfg(target_os = "windows")]
mod windows {
    use std::os::windows::process::CommandExt;
    
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    
    pub fn configure_command(cmd: &mut std::process::Command) {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}
```

### macOS

```rust
#[cfg(target_os = "macos")]
mod macos {
    pub fn check_python_location() -> Option<std::path::PathBuf> {
        // Check common Python locations on macOS
        let locations = [
            "/usr/local/bin/python3",
            "/opt/homebrew/bin/python3",
            "/usr/bin/python3",
        ];
        
        locations.iter()
            .map(|p| std::path::PathBuf::from(p))
            .find(|p| p.exists())
    }
}
```

### Linux

```rust
#[cfg(target_os = "linux")]
mod linux {
    use nix::sys::resource::{setrlimit, Resource};
    
    pub fn set_memory_limit(bytes: u64) -> Result<(), nix::Error> {
        setrlimit(Resource::RLIMIT_AS, bytes, bytes)
    }
}
```

## Build Configuration

### Feature Flags

```toml
[features]
default = ["native-engine"]
native-engine = []
microsandbox-engine = ["microsandbox", "reqwest"]
all-engines = ["native-engine", "microsandbox-engine"]

# Development features
debug-logging = ["tracing", "tracing-subscriber"]
benchmarks = []
```

### Build Script

Create `build.rs`:

```rust
use std::env;
use std::path::PathBuf;

fn main() {
    // Check for Python
    println!("cargo:rerun-if-env-changed=PYTHON_SYS_EXECUTABLE");
    
    if env::var("PYTHON_SYS_EXECUTABLE").is_err() {
        // Try to find Python
        let python_paths = if cfg!(target_os = "windows") {
            vec!["python.exe", "python3.exe"]
        } else {
            vec!["python3", "python"]
        };
        
        for python in python_paths {
            if std::process::Command::new(python)
                .arg("--version")
                .output()
                .is_ok()
            {
                println!("cargo:rustc-env=PYTHON_SYS_EXECUTABLE={}", python);
                break;
            }
        }
    }
}
```

## Testing

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_basic_execution() {
        let sandbox = create_default_sandbox().await.unwrap();
        let result = sandbox.execute(
            "result = 2 + 2",
            serde_json::json!({}),
            ExecutionOptions::default()
        ).await.unwrap();
        
        assert_eq!(result, serde_json::json!(4));
    }
    
    #[tokio::test]
    async fn test_import_restriction() {
        let sandbox = create_default_sandbox().await.unwrap();
        let result = sandbox.execute(
            "import os; result = os.getcwd()",
            serde_json::json!({}),
            ExecutionOptions::default()
        ).await;
        
        assert!(matches!(result, Err(SandboxError::ImportRestriction(_))));
    }
}
```

### Integration Tests

Create `tests/integration_test.rs`:

```rust
use pysandbox::*;

#[tokio::test]
async fn test_data_processing() {
    let sandbox = create_default_sandbox().await.unwrap();
    
    let code = r#"
import json
data = inputs['data']
result = {
    'sum': sum(data),
    'count': len(data),
    'average': sum(data) / len(data) if data else 0
}
"#;
    
    let inputs = serde_json::json!({
        "data": [1, 2, 3, 4, 5]
    });
    
    let result = sandbox.execute(code, inputs, ExecutionOptions::default())
        .await
        .unwrap();
    
    assert_eq!(result["sum"], 15);
    assert_eq!(result["count"], 5);
    assert_eq!(result["average"], 3.0);
}
```

## Deployment

### Static Linking

For easier deployment, statically link Python:

```toml
[dependencies.pyo3]
version = "0.20"
features = ["auto-initialize", "extension-module"]
```

### Bundling Python

Include Python runtime in your binary:

```rust
use include_dir::{include_dir, Dir};

static PYTHON_RUNTIME: Dir = include_dir!("$CARGO_MANIFEST_DIR/python-runtime");

pub fn extract_python_runtime(target_dir: &Path) -> Result<(), std::io::Error> {
    PYTHON_RUNTIME.extract(target_dir)
}
```

## Best Practices

1. **Resource Management**: Always set appropriate resource limits
2. **Error Handling**: Handle all error cases gracefully
3. **Security**: Never execute untrusted code without sandboxing
4. **Performance**: Use pooling for high-throughput scenarios
5. **Monitoring**: Log execution metrics and errors

## Example: Complete Application

```rust
use pysandbox::{create_default_sandbox, ExecutionOptions, ImportPolicy};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Serialize, Deserialize)]
struct CodeExecutionRequest {
    code: String,
    language: String,
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CodeExecutionResponse {
    success: bool,
    output: Option<serde_json::Value>,
    error: Option<String>,
    execution_time_ms: u64,
}

pub struct CodeExecutionService {
    sandbox: Arc<Mutex<PythonSandbox>>,
}

impl CodeExecutionService {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let sandbox = create_default_sandbox().await?;
        Ok(Self {
            sandbox: Arc::new(Mutex::new(sandbox)),
        })
    }
    
    pub async fn execute(&self, request: CodeExecutionRequest) -> CodeExecutionResponse {
        let start = std::time::Instant::now();
        
        if request.language != "python" {
            return CodeExecutionResponse {
                success: false,
                output: None,
                error: Some("Only Python is supported".to_string()),
                execution_time_ms: 0,
            };
        }
        
        let mut options = ExecutionOptions::default();
        if let Some(timeout) = request.timeout_seconds {
            options.timeout = std::time::Duration::from_secs(timeout);
        }
        
        let sandbox = self.sandbox.lock().await;
        match sandbox.execute(&request.code, serde_json::json!({}), options).await {
            Ok(result) => CodeExecutionResponse {
                success: true,
                output: Some(result),
                error: None,
                execution_time_ms: start.elapsed().as_millis() as u64,
            },
            Err(e) => CodeExecutionResponse {
                success: false,
                output: None,
                error: Some(e.to_string()),
                execution_time_ms: start.elapsed().as_millis() as u64,
            },
        }
    }
}
```