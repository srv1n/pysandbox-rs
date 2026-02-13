# Tauri Integration Guide

This guide explains how to embed pysandbox-rs in a Tauri application with dynamic module downloading and management.

## Table of Contents
1. [Overview](#overview)
2. [Project Setup](#project-setup)
3. [Dynamic Module Management](#dynamic-module-management)
4. [Tauri Backend Integration](#tauri-backend-integration)
5. [Frontend API](#frontend-api)
6. [Security Considerations](#security-considerations)
7. [Deployment](#deployment)

## Overview

When integrating pysandbox-rs into a Tauri application, you'll need to:
- Bundle Python runtime with your app
- Dynamically download Python packages as needed
- Expose sandbox functionality through Tauri commands
- Handle security and sandboxing in a desktop environment

## Project Setup

### 1. Add Dependencies

In your Tauri app's `src-tauri/Cargo.toml`:

```toml
[dependencies]
tauri = { version = "1", features = ["shell-open"] }
pysandbox-rs = { path = "../pysandbox-rs", features = ["microsandbox-engine"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
reqwest = { version = "0.12", features = ["stream"] }
futures-util = "0.3"
dirs = "5.0"
zip = "0.6"
which = "6.0"
```

### 2. Project Structure

```
your-tauri-app/
├── src-tauri/
│   ├── src/
│   │   ├── main.rs
│   │   ├── commands.rs         # Tauri commands
│   │   ├── python_manager.rs   # Python runtime management
│   │   ├── module_manager.rs   # Dynamic module downloading
│   │   └── sandbox_service.rs  # Sandbox wrapper service
│   └── Cargo.toml
├── src/                        # Frontend code
└── pysandbox-rs/              # Embedded sandbox library
```

## Dynamic Module Management

### Module Manager Implementation

Create `src-tauri/src/module_manager.rs`:

```rust
use std::path::{Path, PathBuf};
use std::fs;
use reqwest;
use futures_util::StreamExt;
use std::io::Write;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PythonModule {
    pub name: String,
    pub version: String,
    pub installed: bool,
    pub size: Option<u64>,
}

pub struct ModuleManager {
    modules_dir: PathBuf,
    python_executable: PathBuf,
}

impl ModuleManager {
    pub fn new(app_data_dir: PathBuf, python_executable: PathBuf) -> Self {
        let modules_dir = app_data_dir.join("python_modules");
        fs::create_dir_all(&modules_dir).unwrap();
        
        Self {
            modules_dir,
            python_executable,
        }
    }
    
    /// Check if a module is installed
    pub fn is_installed(&self, module: &str) -> bool {
        std::process::Command::new(&self.python_executable)
            .args(&["-c", &format!("import {}", module)])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
    
    /// Install a Python module
    pub async fn install_module(&self, module: &str, version: Option<&str>) -> Result<(), String> {
        let module_spec = if let Some(v) = version {
            format!("{}=={}", module, v)
        } else {
            module.to_string()
        };
        
        println!("Installing Python module: {}", module_spec);
        
        let output = tokio::process::Command::new(&self.python_executable)
            .args(&[
                "-m", "pip", "install",
                "--target", self.modules_dir.to_str().unwrap(),
                "--no-deps",  // Avoid dependency conflicts
                &module_spec
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to run pip: {}", e))?;
        
        if !output.status.success() {
            return Err(format!("Failed to install {}: {}", 
                module_spec, 
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        
        Ok(())
    }
    
    /// Get list of available modules
    pub fn list_installed_modules(&self) -> Vec<PythonModule> {
        let output = std::process::Command::new(&self.python_executable)
            .args(&["-m", "pip", "list", "--format=json"])
            .output();
        
        match output {
            Ok(output) if output.status.success() => {
                let json_str = String::from_utf8_lossy(&output.stdout);
                if let Ok(modules) = serde_json::from_str::<Vec<serde_json::Value>>(&json_str) {
                    modules.into_iter()
                        .filter_map(|m| {
                            Some(PythonModule {
                                name: m.get("name")?.as_str()?.to_string(),
                                version: m.get("version")?.as_str()?.to_string(),
                                installed: true,
                                size: None,
                            })
                        })
                        .collect()
                } else {
                    vec![]
                }
            }
            _ => vec![]
        }
    }
    
    /// Download and cache a pre-built wheel
    pub async fn download_wheel(&self, url: &str, filename: &str) -> Result<PathBuf, String> {
        let wheel_path = self.modules_dir.join("wheels").join(filename);
        fs::create_dir_all(wheel_path.parent().unwrap()).unwrap();
        
        if wheel_path.exists() {
            return Ok(wheel_path);
        }
        
        let response = reqwest::get(url).await
            .map_err(|e| format!("Failed to download wheel: {}", e))?;
        
        let mut file = fs::File::create(&wheel_path)
            .map_err(|e| format!("Failed to create file: {}", e))?;
        
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
            file.write_all(&chunk)
                .map_err(|e| format!("Write error: {}", e))?;
        }
        
        Ok(wheel_path)
    }
}
```

### Python Runtime Manager

Create `src-tauri/src/python_manager.rs`:

```rust
use std::path::{Path, PathBuf};
use std::fs;
use which::which;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PythonInfo {
    pub version: String,
    pub executable: PathBuf,
    pub embedded: bool,
}

pub struct PythonManager {
    app_data_dir: PathBuf,
}

impl PythonManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self { app_data_dir }
    }
    
    /// Find or install Python runtime
    pub async fn ensure_python(&self) -> Result<PythonInfo, String> {
        // First, check for system Python
        if let Ok(python_path) = which("python3") {
            if let Ok(info) = self.get_python_info(&python_path) {
                return Ok(info);
            }
        }
        
        // If no system Python, check for embedded Python
        let embedded_python = self.app_data_dir.join("python");
        if embedded_python.exists() {
            let python_exe = if cfg!(windows) {
                embedded_python.join("python.exe")
            } else {
                embedded_python.join("bin/python3")
            };
            
            if python_exe.exists() {
                if let Ok(info) = self.get_python_info(&python_exe) {
                    return Ok(info);
                }
            }
        }
        
        // Download embedded Python if needed
        self.download_embedded_python().await
    }
    
    /// Get Python version info
    fn get_python_info(&self, python_path: &Path) -> Result<PythonInfo, String> {
        let output = std::process::Command::new(python_path)
            .args(&["--version"])
            .output()
            .map_err(|e| format!("Failed to run Python: {}", e))?;
        
        if !output.status.success() {
            return Err("Python executable failed".to_string());
        }
        
        let version = String::from_utf8_lossy(&output.stdout)
            .trim()
            .replace("Python ", "");
        
        Ok(PythonInfo {
            version,
            executable: python_path.to_path_buf(),
            embedded: python_path.starts_with(&self.app_data_dir),
        })
    }
    
    /// Download embedded Python distribution
    async fn download_embedded_python(&self) -> Result<PythonInfo, String> {
        let python_dir = self.app_data_dir.join("python");
        fs::create_dir_all(&python_dir).unwrap();
        
        // Platform-specific download URLs
        let (url, filename) = if cfg!(target_os = "windows") {
            (
                "https://www.python.org/ftp/python/3.11.7/python-3.11.7-embed-amd64.zip",
                "python-embed.zip"
            )
        } else if cfg!(target_os = "macos") {
            (
                "https://www.python.org/ftp/python/3.11.7/python-3.11.7-macos11.pkg",
                "python-macos.pkg"
            )
        } else {
            return Err("Embedded Python not available for this platform".to_string());
        };
        
        // Download Python
        let download_path = python_dir.join(filename);
        let response = reqwest::get(url).await
            .map_err(|e| format!("Failed to download Python: {}", e))?;
        
        let bytes = response.bytes().await
            .map_err(|e| format!("Failed to read response: {}", e))?;
        
        fs::write(&download_path, bytes)
            .map_err(|e| format!("Failed to save Python: {}", e))?;
        
        // Extract Python
        #[cfg(windows)]
        {
            let file = fs::File::open(&download_path)
                .map_err(|e| format!("Failed to open zip: {}", e))?;
            let mut archive = zip::ZipArchive::new(file)
                .map_err(|e| format!("Failed to read zip: {}", e))?;
            
            for i in 0..archive.len() {
                let mut file = archive.by_index(i).unwrap();
                let outpath = python_dir.join(file.name());
                
                if file.name().ends_with('/') {
                    fs::create_dir_all(&outpath).unwrap();
                } else {
                    if let Some(p) = outpath.parent() {
                        fs::create_dir_all(p).unwrap();
                    }
                    let mut outfile = fs::File::create(&outpath).unwrap();
                    std::io::copy(&mut file, &mut outfile).unwrap();
                }
            }
        }
        
        // Return Python info
        let python_exe = if cfg!(windows) {
            python_dir.join("python.exe")
        } else {
            python_dir.join("bin/python3")
        };
        
        self.get_python_info(&python_exe)
    }
}
```

## Tauri Backend Integration

### Sandbox Service Wrapper

Create `src-tauri/src/sandbox_service.rs`:

```rust
use pysandbox::{PythonSandbox, ExecutionOptions, create_default_sandbox};
use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub code: String,
    pub inputs: serde_json::Value,
    pub timeout_seconds: Option<u64>,
    pub memory_mb: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

pub struct SandboxService {
    sandbox: Arc<Mutex<PythonSandbox>>,
    module_manager: Arc<super::module_manager::ModuleManager>,
}

impl SandboxService {
    pub async fn new(module_manager: Arc<super::module_manager::ModuleManager>) -> Result<Self, String> {
        let sandbox = create_default_sandbox().await
            .map_err(|e| format!("Failed to create sandbox: {}", e))?;
        
        Ok(Self {
            sandbox: Arc::new(Mutex::new(sandbox)),
            module_manager,
        })
    }
    
    pub async fn execute(&self, request: ExecutionRequest) -> ExecutionResult {
        let start = std::time::Instant::now();
        
        // Extract required modules from code
        let required_modules = self.extract_imports(&request.code);
        
        // Check and install missing modules
        for module in required_modules {
            if !self.module_manager.is_installed(&module) {
                if let Err(e) = self.module_manager.install_module(&module, None).await {
                    return ExecutionResult {
                        success: false,
                        result: None,
                        error: Some(format!("Failed to install module {}: {}", module, e)),
                        execution_time_ms: start.elapsed().as_millis() as u64,
                    };
                }
            }
        }
        
        // Prepare execution options
        let mut options = ExecutionOptions::default();
        if let Some(timeout) = request.timeout_seconds {
            options.timeout = std::time::Duration::from_secs(timeout);
        }
        if let Some(memory) = request.memory_mb {
            options.memory_mb = memory;
        }
        
        // Execute code
        let sandbox = self.sandbox.lock().await;
        match sandbox.execute(&request.code, request.inputs, options).await {
            Ok(result) => ExecutionResult {
                success: true,
                result: Some(result),
                error: None,
                execution_time_ms: start.elapsed().as_millis() as u64,
            },
            Err(e) => ExecutionResult {
                success: false,
                result: None,
                error: Some(e.to_string()),
                execution_time_ms: start.elapsed().as_millis() as u64,
            }
        }
    }
    
    /// Extract import statements from Python code
    fn extract_imports(&self, code: &str) -> Vec<String> {
        let mut imports = Vec::new();
        
        for line in code.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("import ") {
                if let Some(module) = trimmed.strip_prefix("import ").and_then(|s| s.split_whitespace().next()) {
                    imports.push(module.split('.').next().unwrap().to_string());
                }
            } else if trimmed.starts_with("from ") {
                if let Some(module) = trimmed.strip_prefix("from ").and_then(|s| s.split_whitespace().next()) {
                    imports.push(module.split('.').next().unwrap().to_string());
                }
            }
        }
        
        imports.sort();
        imports.dedup();
        imports
    }
    
    pub async fn get_capabilities(&self) -> Vec<pysandbox::engine::EngineCapabilities> {
        let sandbox = self.sandbox.lock().await;
        sandbox.capabilities().await
    }
}
```

### Tauri Commands

Create `src-tauri/src/commands.rs`:

```rust
use crate::sandbox_service::{SandboxService, ExecutionRequest, ExecutionResult};
use crate::module_manager::{ModuleManager, PythonModule};
use crate::python_manager::{PythonManager, PythonInfo};
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::State;

pub struct AppState {
    pub sandbox_service: Arc<Mutex<Option<SandboxService>>>,
    pub module_manager: Arc<Mutex<Option<Arc<ModuleManager>>>>,
    pub python_manager: Arc<PythonManager>,
}

#[tauri::command]
pub async fn initialize_sandbox(state: State<'_, AppState>) -> Result<String, String> {
    // Ensure Python is available
    let python_info = state.python_manager.ensure_python().await?;
    
    // Create module manager
    let app_data_dir = dirs::data_dir()
        .ok_or("Failed to get data directory")?
        .join("your-app-name");
    
    let module_manager = Arc::new(ModuleManager::new(
        app_data_dir,
        python_info.executable.clone()
    ));
    
    // Create sandbox service
    let sandbox_service = SandboxService::new(module_manager.clone()).await?;
    
    // Store in state
    *state.module_manager.lock().await = Some(module_manager);
    *state.sandbox_service.lock().await = Some(sandbox_service);
    
    Ok(format!("Sandbox initialized with Python {}", python_info.version))
}

#[tauri::command]
pub async fn execute_python(
    state: State<'_, AppState>,
    request: ExecutionRequest
) -> Result<ExecutionResult, String> {
    let service_lock = state.sandbox_service.lock().await;
    let service = service_lock.as_ref()
        .ok_or("Sandbox not initialized. Call initialize_sandbox first.")?;
    
    Ok(service.execute(request).await)
}

#[tauri::command]
pub async fn install_module(
    state: State<'_, AppState>,
    module: String,
    version: Option<String>
) -> Result<(), String> {
    let manager_lock = state.module_manager.lock().await;
    let manager = manager_lock.as_ref()
        .ok_or("Module manager not initialized")?;
    
    manager.install_module(&module, version.as_deref()).await
}

#[tauri::command]
pub async fn list_installed_modules(
    state: State<'_, AppState>
) -> Result<Vec<PythonModule>, String> {
    let manager_lock = state.module_manager.lock().await;
    let manager = manager_lock.as_ref()
        .ok_or("Module manager not initialized")?;
    
    Ok(manager.list_installed_modules())
}

#[tauri::command]
pub async fn get_python_info(
    state: State<'_, AppState>
) -> Result<PythonInfo, String> {
    state.python_manager.ensure_python().await
}

#[tauri::command]
pub async fn get_sandbox_capabilities(
    state: State<'_, AppState>
) -> Result<Vec<pysandbox::engine::EngineCapabilities>, String> {
    let service_lock = state.sandbox_service.lock().await;
    let service = service_lock.as_ref()
        .ok_or("Sandbox not initialized")?;
    
    Ok(service.get_capabilities().await)
}
```

### Main Tauri Setup

Update your `src-tauri/src/main.rs`:

```rust
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod module_manager;
mod python_manager;
mod sandbox_service;

use std::sync::Arc;
use tokio::sync::Mutex;

fn main() {
    let app_data_dir = dirs::data_dir()
        .expect("Failed to get data directory")
        .join("your-app-name");
    
    std::fs::create_dir_all(&app_data_dir).unwrap();
    
    let app_state = commands::AppState {
        sandbox_service: Arc::new(Mutex::new(None)),
        module_manager: Arc::new(Mutex::new(None)),
        python_manager: Arc::new(python_manager::PythonManager::new(app_data_dir)),
    };
    
    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::initialize_sandbox,
            commands::execute_python,
            commands::install_module,
            commands::list_installed_modules,
            commands::get_python_info,
            commands::get_sandbox_capabilities,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## Frontend API

### TypeScript API Wrapper

Create `src/lib/pythonSandbox.ts`:

```typescript
import { invoke } from '@tauri-apps/api/tauri';

export interface ExecutionRequest {
  code: string;
  inputs?: any;
  timeout_seconds?: number;
  memory_mb?: number;
}

export interface ExecutionResult {
  success: boolean;
  result?: any;
  error?: string;
  execution_time_ms: number;
}

export interface PythonModule {
  name: string;
  version: string;
  installed: boolean;
  size?: number;
}

export interface PythonInfo {
  version: string;
  executable: string;
  embedded: boolean;
}

export interface EngineCapabilities {
  name: string;
  numpy: boolean;
  matplotlib: boolean;
  pandas: boolean;
  max_memory_mb: number;
  max_cpu_seconds: number;
  security_level: number;
}

export class PythonSandbox {
  private initialized = false;

  async initialize(): Promise<string> {
    const result = await invoke<string>('initialize_sandbox');
    this.initialized = true;
    return result;
  }

  async execute(request: ExecutionRequest): Promise<ExecutionResult> {
    if (!this.initialized) {
      throw new Error('Sandbox not initialized. Call initialize() first.');
    }
    return invoke<ExecutionResult>('execute_python', { request });
  }

  async installModule(module: string, version?: string): Promise<void> {
    return invoke('install_module', { module, version });
  }

  async listInstalledModules(): Promise<PythonModule[]> {
    return invoke<PythonModule[]>('list_installed_modules');
  }

  async getPythonInfo(): Promise<PythonInfo> {
    return invoke<PythonInfo>('get_python_info');
  }

  async getCapabilities(): Promise<EngineCapabilities[]> {
    if (!this.initialized) {
      throw new Error('Sandbox not initialized. Call initialize() first.');
    }
    return invoke<EngineCapabilities[]>('get_sandbox_capabilities');
  }
}

// Singleton instance
export const pythonSandbox = new PythonSandbox();
```

### React Component Example

```tsx
import React, { useState, useEffect } from 'react';
import { pythonSandbox, ExecutionResult } from '../lib/pythonSandbox';

export function PythonEditor() {
  const [code, setCode] = useState('');
  const [result, setResult] = useState<ExecutionResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [initialized, setInitialized] = useState(false);

  useEffect(() => {
    // Initialize sandbox on mount
    pythonSandbox.initialize()
      .then((message) => {
        console.log(message);
        setInitialized(true);
      })
      .catch(console.error);
  }, []);

  const runCode = async () => {
    if (!initialized) return;
    
    setLoading(true);
    try {
      const result = await pythonSandbox.execute({
        code,
        inputs: {},
        timeout_seconds: 30,
        memory_mb: 512
      });
      setResult(result);
    } catch (error) {
      setResult({
        success: false,
        error: error.toString(),
        result: null,
        execution_time_ms: 0
      });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div>
      <textarea
        value={code}
        onChange={(e) => setCode(e.target.value)}
        placeholder="Enter Python code..."
        rows={10}
        cols={50}
      />
      <button onClick={runCode} disabled={!initialized || loading}>
        {loading ? 'Running...' : 'Run'}
      </button>
      
      {result && (
        <div>
          <h3>Result:</h3>
          {result.success ? (
            <pre>{JSON.stringify(result.result, null, 2)}</pre>
          ) : (
            <div style={{ color: 'red' }}>{result.error}</div>
          )}
          <small>Execution time: {result.execution_time_ms}ms</small>
        </div>
      )}
    </div>
  );
}
```

## Security Considerations

### 1. Sandbox Configuration

Always use appropriate security settings:

```rust
// In your sandbox_service.rs
use pysandbox::{ImportPolicy, ResourceLimits};

let options = ExecutionOptions {
    memory_mb: 512,  // Limit memory
    cpu_seconds: 30,  // Limit CPU time
    timeout: Duration::from_secs(35),
    import_policy: ImportPolicy::default(),  // Blacklist dangerous modules
    env_vars: HashMap::new(),
};
```

### 2. CSP Configuration

Update your Tauri security configuration in `tauri.conf.json`:

```json
{
  "tauri": {
    "security": {
      "csp": {
        "default-src": "'self'",
        "script-src": "'self' 'unsafe-inline'",
        "style-src": "'self' 'unsafe-inline'"
      }
    },
    "allowlist": {
      "shell": {
        "execute": false,
        "open": false
      }
    }
  }
}
```

### 3. Input Validation

Always validate user input before execution:

```rust
fn validate_code(code: &str) -> Result<(), String> {
    // Check code length
    if code.len() > 10_000 {
        return Err("Code too long".to_string());
    }
    
    // Check for suspicious patterns
    let suspicious_patterns = [
        "__import__",
        "eval(",
        "exec(",
        "compile(",
        "globals(",
        "locals(",
    ];
    
    for pattern in &suspicious_patterns {
        if code.contains(pattern) {
            return Err(format!("Suspicious pattern detected: {}", pattern));
        }
    }
    
    Ok(())
}
```

## Deployment

### 1. Bundle Python Runtime

For Windows, include embedded Python in your installer:

```toml
# tauri.conf.json
{
  "tauri": {
    "bundle": {
      "resources": [
        "python/**/*"
      ]
    }
  }
}
```

### 2. Code Signing

Sign your application to avoid security warnings:

```bash
# macOS
codesign --deep --force --verify --verbose --sign "Developer ID Application: Your Name" target/release/bundle/macos/YourApp.app

# Windows
signtool sign /fd SHA256 /a target/release/YourApp.exe
```

### 3. Auto-updater Configuration

Set up auto-updates for security patches:

```json
{
  "tauri": {
    "updater": {
      "active": true,
      "endpoints": [
        "https://your-domain.com/update/{{target}}/{{current_version}}"
      ],
      "dialog": true,
      "pubkey": "YOUR_PUBLIC_KEY"
    }
  }
}
```

## Best Practices

1. **Lazy Loading**: Only initialize the sandbox when needed
2. **Module Caching**: Cache downloaded modules locally
3. **Progress Feedback**: Show download progress for modules
4. **Error Recovery**: Implement fallback mechanisms
5. **Resource Cleanup**: Properly cleanup sandboxes after use

## Troubleshooting

### Common Issues

1. **Python not found**: Ensure Python 3.8+ is installed or bundle embedded Python
2. **Module installation fails**: Check network connectivity and pip configuration
3. **Sandbox initialization fails**: Verify microsandbox server is running (if using that engine)
4. **Memory errors**: Increase memory limits in ExecutionOptions

### Debug Mode

Enable debug logging:

```rust
env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
```

## Examples

See the `examples/tauri-app` directory for a complete working example.