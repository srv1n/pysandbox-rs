# Dynamic Module Management Guide

This guide explains how to dynamically download and manage Python modules in your application.

## Overview

Dynamic module management allows your application to:
- Download Python packages on-demand
- Cache modules for offline use
- Manage module versions
- Handle dependencies safely
- Provide progress feedback to users

## Architecture

```
┌─────────────────┐
│   User Request  │
│  "import numpy" │
└────────┬────────┘
         │
┌────────▼────────┐
│ Module Manager  │
│  - Check cache  │
│  - Download     │
│  - Install      │
└────────┬────────┘
         │
┌────────▼────────┐
│ Python Sandbox  │
│  - Execute code │
│  - Import module│
└─────────────────┘
```

## Implementation

### 1. Module Registry

Create a registry of approved modules with metadata:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModuleInfo {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub size_mb: f64,
    pub dependencies: Vec<String>,
    pub wheel_url: Option<String>,
    pub pypi_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModuleRegistry {
    modules: HashMap<String, ModuleInfo>,
}

impl ModuleRegistry {
    pub fn default() -> Self {
        let mut modules = HashMap::new();
        
        // Data science modules
        modules.insert("numpy".to_string(), ModuleInfo {
            name: "numpy".to_string(),
            display_name: "NumPy".to_string(),
            description: "Fundamental package for scientific computing".to_string(),
            version: "1.26.2".to_string(),
            size_mb: 18.5,
            dependencies: vec![],
            wheel_url: None,
            pypi_name: Some("numpy".to_string()),
        });
        
        modules.insert("pandas".to_string(), ModuleInfo {
            name: "pandas".to_string(),
            display_name: "Pandas".to_string(),
            description: "Data analysis and manipulation tool".to_string(),
            version: "2.1.4".to_string(),
            size_mb: 32.1,
            dependencies: vec!["numpy".to_string(), "python-dateutil".to_string(), "pytz".to_string()],
            wheel_url: None,
            pypi_name: Some("pandas".to_string()),
        });
        
        modules.insert("matplotlib".to_string(), ModuleInfo {
            name: "matplotlib".to_string(),
            display_name: "Matplotlib".to_string(),
            description: "Comprehensive library for creating visualizations".to_string(),
            version: "3.8.2".to_string(),
            size_mb: 45.3,
            dependencies: vec!["numpy".to_string(), "pillow".to_string()],
            wheel_url: None,
            pypi_name: Some("matplotlib".to_string()),
        });
        
        modules.insert("scipy".to_string(), ModuleInfo {
            name: "scipy".to_string(),
            display_name: "SciPy".to_string(),
            description: "Scientific computing tools".to_string(),
            version: "1.11.4".to_string(),
            size_mb: 38.7,
            dependencies: vec!["numpy".to_string()],
            wheel_url: None,
            pypi_name: Some("scipy".to_string()),
        });
        
        modules.insert("scikit-learn".to_string(), ModuleInfo {
            name: "scikit-learn".to_string(),
            display_name: "Scikit-learn".to_string(),
            description: "Machine learning library".to_string(),
            version: "1.3.2".to_string(),
            size_mb: 25.8,
            dependencies: vec!["numpy".to_string(), "scipy".to_string()],
            wheel_url: None,
            pypi_name: Some("scikit-learn".to_string()),
        });
        
        Self { modules }
    }
    
    pub fn get_module(&self, name: &str) -> Option<&ModuleInfo> {
        self.modules.get(name)
    }
    
    pub fn list_modules(&self) -> Vec<&ModuleInfo> {
        self.modules.values().collect()
    }
    
    pub fn get_dependencies(&self, module: &str) -> Vec<String> {
        let mut deps = Vec::new();
        let mut to_process = vec![module.to_string()];
        let mut processed = std::collections::HashSet::new();
        
        while let Some(current) = to_process.pop() {
            if processed.contains(&current) {
                continue;
            }
            processed.insert(current.clone());
            
            if let Some(info) = self.modules.get(&current) {
                for dep in &info.dependencies {
                    if !processed.contains(dep) {
                        deps.push(dep.clone());
                        to_process.push(dep.clone());
                    }
                }
            }
        }
        
        deps.reverse(); // Install dependencies first
        deps
    }
}
```

### 2. Smart Module Downloader

Implement a downloader with progress tracking and caching:

```rust
use tokio::fs;
use tokio::io::AsyncWriteExt;
use futures_util::StreamExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub module: String,
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub percentage: f32,
}

pub struct ModuleDownloader {
    cache_dir: PathBuf,
    registry: ModuleRegistry,
    progress_sender: Option<tokio::sync::mpsc::Sender<DownloadProgress>>,
}

impl ModuleDownloader {
    pub fn new(cache_dir: PathBuf) -> Self {
        fs::create_dir_all(&cache_dir).await.unwrap();
        
        Self {
            cache_dir,
            registry: ModuleRegistry::default(),
            progress_sender: None,
        }
    }
    
    pub fn set_progress_callback(&mut self, sender: tokio::sync::mpsc::Sender<DownloadProgress>) {
        self.progress_sender = Some(sender);
    }
    
    pub async fn ensure_module(&self, module: &str) -> Result<PathBuf, String> {
        // Check if already cached
        let module_dir = self.cache_dir.join(module);
        if module_dir.exists() {
            return Ok(module_dir);
        }
        
        // Get module info
        let info = self.registry.get_module(module)
            .ok_or_else(|| format!("Unknown module: {}", module))?;
        
        // Download dependencies first
        let deps = self.registry.get_dependencies(module);
        for dep in deps {
            self.ensure_module(&dep).await?;
        }
        
        // Download module
        self.download_module(info).await
    }
    
    async fn download_module(&self, info: &ModuleInfo) -> Result<PathBuf, String> {
        let module_dir = self.cache_dir.join(&info.name);
        fs::create_dir_all(&module_dir).await
            .map_err(|e| format!("Failed to create module directory: {}", e))?;
        
        // Use wheel URL if provided, otherwise use PyPI
        let download_url = if let Some(wheel_url) = &info.wheel_url {
            wheel_url.clone()
        } else if let Some(pypi_name) = &info.pypi_name {
            // Use PyPI JSON API to get download URL
            self.get_pypi_wheel_url(pypi_name, &info.version).await?
        } else {
            return Err("No download source available".to_string());
        };
        
        // Download the wheel
        let wheel_path = module_dir.join(format!("{}-{}.whl", info.name, info.version));
        self.download_file(&download_url, &wheel_path, &info.name).await?;
        
        // Extract wheel
        self.extract_wheel(&wheel_path, &module_dir).await?;
        
        Ok(module_dir)
    }
    
    async fn get_pypi_wheel_url(&self, package: &str, version: &str) -> Result<String, String> {
        let url = format!("https://pypi.org/pypi/{}/{}/json", package, version);
        let response = reqwest::get(&url).await
            .map_err(|e| format!("Failed to fetch PyPI metadata: {}", e))?;
        
        let json: serde_json::Value = response.json().await
            .map_err(|e| format!("Failed to parse PyPI response: {}", e))?;
        
        // Find appropriate wheel for platform
        let urls = json["urls"].as_array()
            .ok_or("No download URLs found")?;
        
        // Prefer platform-specific wheel, fallback to pure Python
        let wheel = urls.iter()
            .find(|u| {
                let filename = u["filename"].as_str().unwrap_or("");
                filename.ends_with(".whl") && 
                (filename.contains("none-any") || self.matches_platform(filename))
            })
            .or_else(|| urls.iter().find(|u| u["filename"].as_str().unwrap_or("").ends_with(".whl")))
            .ok_or("No suitable wheel found")?;
        
        wheel["url"].as_str()
            .ok_or("Invalid wheel URL")
            .map(|s| s.to_string())
    }
    
    fn matches_platform(&self, filename: &str) -> bool {
        #[cfg(target_os = "windows")]
        return filename.contains("win_amd64") || filename.contains("win32");
        
        #[cfg(target_os = "macos")]
        return filename.contains("macosx") && 
               (filename.contains("x86_64") || filename.contains("universal2"));
        
        #[cfg(target_os = "linux")]
        return filename.contains("linux") && filename.contains("x86_64");
        
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        return false;
    }
    
    async fn download_file(&self, url: &str, path: &Path, module: &str) -> Result<(), String> {
        let response = reqwest::get(url).await
            .map_err(|e| format!("Failed to download: {}", e))?;
        
        let total_size = response
            .content_length()
            .ok_or("Unknown content length")?;
        
        let mut file = fs::File::create(path).await
            .map_err(|e| format!("Failed to create file: {}", e))?;
        
        let mut downloaded = 0u64;
        let mut stream = response.bytes_stream();
        
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
            downloaded += chunk.len() as u64;
            
            file.write_all(&chunk).await
                .map_err(|e| format!("Write error: {}", e))?;
            
            // Send progress update
            if let Some(sender) = &self.progress_sender {
                let progress = DownloadProgress {
                    module: module.to_string(),
                    bytes_downloaded: downloaded,
                    total_bytes: total_size,
                    percentage: (downloaded as f32 / total_size as f32) * 100.0,
                };
                let _ = sender.send(progress).await;
            }
        }
        
        Ok(())
    }
    
    async fn extract_wheel(&self, wheel_path: &Path, target_dir: &Path) -> Result<(), String> {
        let file = std::fs::File::open(wheel_path)
            .map_err(|e| format!("Failed to open wheel: {}", e))?;
        
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("Failed to read wheel: {}", e))?;
        
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)
                .map_err(|e| format!("Failed to read archive entry: {}", e))?;
            
            let outpath = target_dir.join(file.name());
            
            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath).await
                    .map_err(|e| format!("Failed to create directory: {}", e))?;
            } else {
                if let Some(p) = outpath.parent() {
                    fs::create_dir_all(p).await
                        .map_err(|e| format!("Failed to create parent directory: {}", e))?;
                }
                
                let mut outfile = fs::File::create(&outpath).await
                    .map_err(|e| format!("Failed to create file: {}", e))?;
                
                let mut buffer = Vec::new();
                std::io::Read::read_to_end(&mut file, &mut buffer)
                    .map_err(|e| format!("Failed to read file: {}", e))?;
                
                outfile.write_all(&buffer).await
                    .map_err(|e| format!("Failed to write file: {}", e))?;
            }
        }
        
        Ok(())
    }
}
```

### 3. Integration with Sandbox

Update the sandbox to use cached modules:

```rust
impl SandboxService {
    pub async fn execute_with_modules(&self, request: ExecutionRequest) -> ExecutionResult {
        // Add module paths to Python path
        let module_paths = self.get_module_paths().await;
        
        let setup_code = format!(
            r#"
import sys
import os

# Add module paths
module_paths = {}
for path in module_paths:
    if path not in sys.path:
        sys.path.insert(0, path)

# User code below
{}"#,
            serde_json::to_string(&module_paths).unwrap(),
            request.code
        );
        
        // Execute with modified code
        let mut modified_request = request;
        modified_request.code = setup_code;
        
        self.execute(modified_request).await
    }
    
    async fn get_module_paths(&self) -> Vec<String> {
        let cache_dir = self.module_manager.cache_dir.clone();
        let mut paths = vec![];
        
        if let Ok(entries) = fs::read_dir(&cache_dir).await {
            let mut entries = entries;
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                    paths.push(entry.path().to_string_lossy().to_string());
                }
            }
        }
        
        paths
    }
}
```

## Usage Examples

### Basic Module Installation

```typescript
// Frontend code
import { pythonSandbox } from './lib/pythonSandbox';

async function installDataScienceStack() {
  const modules = ['numpy', 'pandas', 'matplotlib', 'scipy'];
  
  for (const module of modules) {
    console.log(`Installing ${module}...`);
    await pythonSandbox.installModule(module);
  }
  
  console.log('Data science stack installed!');
}
```

### Auto-installation on Import

```typescript
async function executeWithAutoInstall(code: string) {
  // Extract imports from code
  const imports = extractImports(code);
  
  // Check and install missing modules
  const installed = await pythonSandbox.listInstalledModules();
  const installedNames = new Set(installed.map(m => m.name));
  
  for (const module of imports) {
    if (!installedNames.has(module)) {
      console.log(`Installing missing module: ${module}`);
      await pythonSandbox.installModule(module);
    }
  }
  
  // Execute code
  return pythonSandbox.execute({ code });
}

function extractImports(code: string): string[] {
  const imports = new Set<string>();
  const lines = code.split('\n');
  
  for (const line of lines) {
    const trimmed = line.trim();
    
    // Match "import module" or "from module import ..."
    const importMatch = trimmed.match(/^import\s+(\w+)/);
    const fromMatch = trimmed.match(/^from\s+(\w+)/);
    
    if (importMatch) {
      imports.add(importMatch[1]);
    } else if (fromMatch) {
      imports.add(fromMatch[1]);
    }
  }
  
  return Array.from(imports);
}
```

### Progress Tracking UI

```tsx
import React, { useState, useEffect } from 'react';

interface DownloadProgress {
  module: string;
  percentage: number;
  bytesDownloaded: number;
  totalBytes: number;
}

export function ModuleInstaller() {
  const [progress, setProgress] = useState<Map<string, DownloadProgress>>(new Map());
  const [installing, setInstalling] = useState(false);
  
  useEffect(() => {
    // Listen for progress updates
    window.addEventListener('module-download-progress', (event: CustomEvent) => {
      const { module, percentage, bytesDownloaded, totalBytes } = event.detail;
      setProgress(prev => new Map(prev).set(module, {
        module,
        percentage,
        bytesDownloaded,
        totalBytes
      }));
    });
  }, []);
  
  const installModule = async (module: string) => {
    setInstalling(true);
    try {
      await pythonSandbox.installModule(module);
      // Remove from progress when done
      setProgress(prev => {
        const next = new Map(prev);
        next.delete(module);
        return next;
      });
    } catch (error) {
      console.error('Installation failed:', error);
    } finally {
      setInstalling(false);
    }
  };
  
  return (
    <div>
      <h3>Python Modules</h3>
      
      {Array.from(progress.values()).map(p => (
        <div key={p.module}>
          <div>{p.module}</div>
          <progress value={p.percentage} max={100} />
          <span>{p.percentage.toFixed(1)}%</span>
        </div>
      ))}
      
      <button 
        onClick={() => installModule('numpy')}
        disabled={installing}
      >
        Install NumPy
      </button>
    </div>
  );
}
```

## Offline Support

### Pre-bundled Modules

For offline support, bundle commonly used modules:

```rust
pub struct OfflineModuleBundle {
    bundle_path: PathBuf,
    modules: Vec<String>,
}

impl OfflineModuleBundle {
    pub fn new(app_dir: &Path) -> Self {
        let bundle_path = app_dir.join("bundled_modules");
        
        Self {
            bundle_path,
            modules: vec![
                "numpy".to_string(),
                "pandas".to_string(),
                "matplotlib".to_string(),
            ],
        }
    }
    
    pub async fn extract_if_needed(&self, cache_dir: &Path) -> Result<(), String> {
        for module in &self.modules {
            let target = cache_dir.join(module);
            if !target.exists() {
                let source = self.bundle_path.join(format!("{}.zip", module));
                if source.exists() {
                    self.extract_bundled_module(&source, &target).await?;
                }
            }
        }
        Ok(())
    }
}
```

## Security Considerations

### 1. Module Allowlist

Only allow approved modules:

```rust
const ALLOWED_MODULES: &[&str] = &[
    "numpy", "pandas", "matplotlib", "scipy", "scikit-learn",
    "requests", "pillow", "openpyxl", "plotly", "seaborn"
];

fn is_module_allowed(module: &str) -> bool {
    ALLOWED_MODULES.contains(&module)
}
```

### 2. Integrity Verification

Verify downloaded modules:

```rust
use sha2::{Sha256, Digest};

async fn verify_module_integrity(path: &Path, expected_hash: &str) -> Result<bool, String> {
    let contents = fs::read(path).await
        .map_err(|e| format!("Failed to read file: {}", e))?;
    
    let mut hasher = Sha256::new();
    hasher.update(&contents);
    let result = hasher.finalize();
    let hash = format!("{:x}", result);
    
    Ok(hash == expected_hash)
}
```

### 3. Sandboxed Installation

Install modules in isolated environment:

```rust
async fn install_module_sandboxed(module: &str, target_dir: &Path) -> Result<(), String> {
    // Create temporary venv
    let temp_venv = tempfile::tempdir()
        .map_err(|e| format!("Failed to create temp dir: {}", e))?;
    
    // Create virtual environment
    Command::new("python3")
        .args(&["-m", "venv", temp_venv.path().to_str().unwrap()])
        .output()
        .await?;
    
    // Install module in venv
    let pip = temp_venv.path().join("bin/pip");
    Command::new(pip)
        .args(&["install", "--no-deps", module])
        .output()
        .await?;
    
    // Copy to target directory
    let site_packages = temp_venv.path().join("lib/python*/site-packages");
    fs::copy_dir_all(site_packages, target_dir).await?;
    
    Ok(())
}
```

## Performance Optimization

### 1. Parallel Downloads

Download multiple modules concurrently:

```rust
use futures::future::join_all;

async fn download_modules_parallel(modules: Vec<&str>) -> Vec<Result<PathBuf, String>> {
    let downloads = modules.into_iter()
        .map(|module| async move {
            self.ensure_module(module).await
        });
    
    join_all(downloads).await
}
```

### 2. Module Preloading

Preload commonly used modules:

```rust
pub async fn preload_common_modules(&self) {
    let common_modules = ["numpy", "pandas", "matplotlib"];
    
    for module in common_modules {
        let _ = self.ensure_module(module).await;
    }
}
```

## Error Handling

Handle various failure scenarios:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Module not found: {0}")]
    ModuleNotFound(String),
    
    #[error("Incompatible platform: {0}")]
    PlatformError(String),
    
    #[error("Installation failed: {0}")]
    InstallationError(String),
    
    #[error("Integrity check failed")]
    IntegrityError,
}
```

## Testing

Test module management:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_module_download() {
        let temp_dir = tempfile::tempdir().unwrap();
        let downloader = ModuleDownloader::new(temp_dir.path().to_path_buf());
        
        let result = downloader.ensure_module("numpy").await;
        assert!(result.is_ok());
        
        // Verify module is cached
        let numpy_dir = temp_dir.path().join("numpy");
        assert!(numpy_dir.exists());
    }
    
    #[tokio::test]
    async fn test_dependency_resolution() {
        let registry = ModuleRegistry::default();
        let deps = registry.get_dependencies("pandas");
        
        assert!(deps.contains(&"numpy".to_string()));
    }
}
```