# Python Bundle Build Specification

> Instructions for building the bundled Python runtime for Rzn desktop app

## Overview

We need to build a standalone Python distribution with pre-installed packages that ships inside the Tauri app bundle. This enables Skills to run Python code without users installing anything.

## Target Bundles

| Platform | Architecture | Priority | Status |
|----------|--------------|----------|--------|
| macOS | arm64 (Apple Silicon) | P0 | 🔨 Build first |
| macOS | x86_64 (Intel) | P1 | After ARM works |
| Windows | x86_64 | P2 | After macOS |
| Linux | x86_64 | P3 | After Windows |

---

## Phase 1: macOS ARM64 Build

### 1.1 Prerequisites

```bash
# On an Apple Silicon Mac
brew install gnu-tar zstd

# Verify architecture
uname -m  # Should output: arm64
```

### 1.2 Download python-build-standalone

```bash
# Create working directory
mkdir -p ~/rzn-python-bundle && cd ~/rzn-python-bundle

# Download Python 3.11 standalone (install_only variant - smaller)
# Check latest release: https://github.com/indygreg/python-build-standalone/releases
curl -LO https://github.com/indygreg/python-build-standalone/releases/download/20241016/cpython-3.11.10+20241016-aarch64-apple-darwin-install_only.tar.gz

# Extract
tar -xzf cpython-3.11.10+20241016-aarch64-apple-darwin-install_only.tar.gz

# Rename for clarity
mv python rzn-python
```

### 1.3 Install Required Packages

```bash
# Use the bundled pip to install packages
./rzn-python/bin/pip3 install --no-cache-dir \
    pymupdf==1.24.14 \
    numpy==1.26.4 \
    pandas==2.2.3 \
    matplotlib==3.9.2

# Verify installations
./rzn-python/bin/python3 -c "import fitz; print(f'PyMuPDF: {fitz.version}')"
./rzn-python/bin/python3 -c "import numpy; print(f'NumPy: {numpy.__version__}')"
./rzn-python/bin/python3 -c "import pandas; print(f'Pandas: {pandas.__version__}')"
./rzn-python/bin/python3 -c "import matplotlib; print(f'Matplotlib: {matplotlib.__version__}')"
```

### 1.4 Cleanup to Reduce Size

```bash
cd rzn-python

# Remove unnecessary files
rm -rf share/man
rm -rf lib/python3.11/test
rm -rf lib/python3.11/unittest
rm -rf lib/python3.11/idlelib
rm -rf lib/python3.11/tkinter
rm -rf lib/python3.11/turtledemo
rm -rf lib/python3.11/ensurepip  # We don't need pip bootstrapping
find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
find . -type f -name "*.pyc" -delete
find . -type f -name "*.pyo" -delete

# Remove pip/setuptools caches from site-packages
rm -rf lib/python3.11/site-packages/pip
rm -rf lib/python3.11/site-packages/setuptools
rm -rf lib/python3.11/site-packages/*.dist-info/RECORD

cd ..
```

### 1.5 Measure and Package

```bash
# Check uncompressed size
du -sh rzn-python/

# Create compressed archive for distribution testing
tar -czf rzn-python-macos-arm64.tar.gz rzn-python/

# Check compressed size
ls -lh rzn-python-macos-arm64.tar.gz

# Expected sizes:
# - Uncompressed: ~250-300 MB
# - Compressed: ~80-100 MB
```

### 1.6 Test the Bundle

Create a test script:

```bash
cat > test_bundle.py << 'EOF'
#!/usr/bin/env python3
"""Test script for bundled Python environment"""

import sys
print(f"Python: {sys.version}")
print(f"Executable: {sys.executable}")
print()

# Test PyMuPDF
try:
    import fitz
    print(f"✅ PyMuPDF {fitz.version}")
except ImportError as e:
    print(f"❌ PyMuPDF: {e}")

# Test NumPy
try:
    import numpy as np
    arr = np.array([1, 2, 3, 4, 5])
    print(f"✅ NumPy {np.__version__} - sum test: {arr.sum()}")
except ImportError as e:
    print(f"❌ NumPy: {e}")

# Test Pandas
try:
    import pandas as pd
    df = pd.DataFrame({'a': [1, 2, 3], 'b': [4, 5, 6]})
    print(f"✅ Pandas {pd.__version__} - DataFrame shape: {df.shape}")
except ImportError as e:
    print(f"❌ Pandas: {e}")

# Test Matplotlib
try:
    import matplotlib
    matplotlib.use('Agg')  # Non-GUI backend
    import matplotlib.pyplot as plt
    plt.figure()
    plt.plot([1, 2, 3], [1, 4, 9])
    plt.savefig('/tmp/test_plot.png')
    print(f"✅ Matplotlib {matplotlib.__version__} - plot saved to /tmp/test_plot.png")
except ImportError as e:
    print(f"❌ Matplotlib: {e}")

print()
print("Bundle test complete!")
EOF

# Run test
./rzn-python/bin/python3 test_bundle.py
```

---

## Phase 2: Integration with pysandbox-rs

### 2.1 Required Code Changes

The `NativePythonEngine` currently finds Python via `which`. We need to add support for a custom path.

**File: `src/native.rs`**

Add a new constructor:

```rust
impl NativePythonEngine {
    /// Create with explicit Python path (for bundled Python)
    pub fn with_python_path(python_path: PathBuf) -> Result<Self> {
        if !python_path.exists() {
            return Err(SandboxError::PythonNotFound);
        }

        let temp_dir = std::env::temp_dir().join("pysandbox");
        std::fs::create_dir_all(&temp_dir)?;

        Ok(Self {
            python_path,
            temp_dir,
            limits: ResourceLimits::default(),
        })
    }

    /// Create with bundled Python (Tauri resource)
    pub fn with_bundled_python(app_handle: &tauri::AppHandle) -> Result<Self> {
        let python_path = app_handle
            .path_resolver()
            .resolve_resource("python/bin/python3")
            .ok_or(SandboxError::PythonNotFound)?;

        Self::with_python_path(python_path)
    }
}
```

**File: `src/sandbox_builder.rs`**

Add bundled Python support:

```rust
/// Options for creating a sandbox
pub struct SandboxOptions {
    pub interactive_setup: bool,
    pub prefer_microsandbox: bool,
    /// Custom Python path (for bundled Python)
    pub python_path: Option<PathBuf>,
}

/// Create sandbox with bundled Python
pub async fn create_bundled_sandbox(python_path: PathBuf) -> Result<PythonSandbox> {
    let engine = NativePythonEngine::with_python_path(python_path)?;
    Ok(PythonSandbox::new(vec![Box::new(engine)]))
}
```

### 2.2 Tauri Integration

**File: `src-tauri/tauri.conf.json`**

Add Python bundle to resources:

```json
{
  "bundle": {
    "resources": [
      "python/**/*"
    ]
  }
}
```

**File: `src-tauri/src/commands/python_sandbox.rs`**

```rust
use pysandbox::{PythonSandbox, ExecutionOptions, NativePythonEngine};
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::Manager;

pub struct PythonState {
    sandbox: Arc<Mutex<Option<PythonSandbox>>>,
}

impl PythonState {
    pub fn new() -> Self {
        Self {
            sandbox: Arc::new(Mutex::new(None)),
        }
    }
}

#[tauri::command]
pub async fn execute_python_skill(
    app: tauri::AppHandle,
    state: tauri::State<'_, PythonState>,
    code: String,
    inputs: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut sandbox_lock = state.sandbox.lock().await;

    // Initialize sandbox with bundled Python if not already done
    if sandbox_lock.is_none() {
        let python_path = app
            .path_resolver()
            .resolve_resource("python/bin/python3")
            .ok_or("Bundled Python not found")?;

        let engine = NativePythonEngine::with_python_path(python_path)
            .map_err(|e| format!("Failed to create Python engine: {}", e))?;

        *sandbox_lock = Some(PythonSandbox::new(vec![Box::new(engine)]));
    }

    let sandbox = sandbox_lock.as_ref().unwrap();
    let options = ExecutionOptions::default();

    sandbox
        .execute(&code, inputs, options)
        .await
        .map_err(|e| e.to_string())
}
```

---

## Phase 3: Directory Structure

### 3.1 In Development

```
rznapp/
├── src-tauri/
│   ├── python/                    # Bundled Python (gitignored)
│   │   ├── bin/
│   │   │   ├── python3
│   │   │   └── python3.11
│   │   └── lib/
│   │       └── python3.11/
│   │           └── site-packages/
│   │               ├── fitz/      # PyMuPDF
│   │               ├── numpy/
│   │               ├── pandas/
│   │               └── matplotlib/
│   ├── tauri.conf.json
│   └── src/
└── pysandbox-rs/                  # Sandbox library
```

### 3.2 In Built App (macOS)

```
Rzn.app/
└── Contents/
    ├── MacOS/
    │   └── rzn                    # Main binary
    └── Resources/
        └── python/                # Bundled Python
            ├── bin/
            │   └── python3
            └── lib/
                └── python3.11/
                    └── site-packages/
```

---

## Phase 4: Build Script

Create `scripts/build-python-bundle.sh`:

```bash
#!/bin/bash
set -e

# Configuration
PYTHON_VERSION="3.11.10"
PYTHON_BUILD_DATE="20241016"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$PROJECT_ROOT/src-tauri/python"

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    arm64|aarch64)
        ARCH_STRING="aarch64-apple-darwin"
        ;;
    x86_64)
        ARCH_STRING="x86_64-apple-darwin"
        ;;
    *)
        echo "❌ Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

echo "🐍 Building Python bundle for $ARCH_STRING"

# Create temp directory
TEMP_DIR=$(mktemp -d)
cd "$TEMP_DIR"

# Download python-build-standalone
PYTHON_URL="https://github.com/indygreg/python-build-standalone/releases/download/${PYTHON_BUILD_DATE}/cpython-${PYTHON_VERSION}+${PYTHON_BUILD_DATE}-${ARCH_STRING}-install_only.tar.gz"
echo "📥 Downloading from $PYTHON_URL"
curl -L -o python.tar.gz "$PYTHON_URL"

# Extract
echo "📦 Extracting..."
tar -xzf python.tar.gz

# Install packages
echo "📚 Installing packages..."
./python/bin/pip3 install --no-cache-dir \
    pymupdf==1.24.14 \
    numpy==1.26.4 \
    pandas==2.2.3 \
    matplotlib==3.9.2

# Cleanup
echo "🧹 Cleaning up..."
cd python
rm -rf share/man lib/python3.11/test lib/python3.11/unittest \
       lib/python3.11/idlelib lib/python3.11/tkinter lib/python3.11/turtledemo \
       lib/python3.11/ensurepip
find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
find . -type f -name "*.pyc" -delete
find . -type f -name "*.pyo" -delete
rm -rf lib/python3.11/site-packages/pip lib/python3.11/site-packages/setuptools
cd ..

# Copy to output
echo "📁 Copying to $OUTPUT_DIR"
rm -rf "$OUTPUT_DIR"
mv python "$OUTPUT_DIR"

# Verify
echo "✅ Verifying installation..."
"$OUTPUT_DIR/bin/python3" -c "
import fitz, numpy, pandas, matplotlib
print(f'PyMuPDF: {fitz.version}')
print(f'NumPy: {numpy.__version__}')
print(f'Pandas: {pandas.__version__}')
print(f'Matplotlib: {matplotlib.__version__}')
"

# Report sizes
UNCOMPRESSED_SIZE=$(du -sh "$OUTPUT_DIR" | cut -f1)
echo ""
echo "📊 Bundle Statistics:"
echo "   Uncompressed size: $UNCOMPRESSED_SIZE"
echo "   Location: $OUTPUT_DIR"

# Cleanup temp
rm -rf "$TEMP_DIR"

echo ""
echo "✅ Python bundle ready!"
echo "   Run 'cargo tauri build' to include in app bundle"
```

Make it executable:

```bash
chmod +x scripts/build-python-bundle.sh
```

---

## Phase 5: CI/CD Integration

### 5.1 GitHub Actions Workflow

Create `.github/workflows/build-python-bundle.yml`:

```yaml
name: Build Python Bundle

on:
  workflow_dispatch:
    inputs:
      platform:
        description: 'Target platform'
        required: true
        default: 'macos-arm64'
        type: choice
        options:
          - macos-arm64
          - macos-x64
          - windows-x64
          - linux-x64
          - all

jobs:
  build-macos-arm64:
    if: ${{ github.event.inputs.platform == 'macos-arm64' || github.event.inputs.platform == 'all' }}
    runs-on: macos-14  # M1 runner
    steps:
      - uses: actions/checkout@v4

      - name: Build Python bundle
        run: ./scripts/build-python-bundle.sh

      - name: Test bundle
        run: |
          ./src-tauri/python/bin/python3 -c "
          import fitz, numpy, pandas, matplotlib
          print('All packages imported successfully')
          "

      - name: Package bundle
        run: |
          cd src-tauri
          tar -czf python-bundle-macos-arm64.tar.gz python/

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: python-bundle-macos-arm64
          path: src-tauri/python-bundle-macos-arm64.tar.gz
          retention-days: 30

  build-macos-x64:
    if: ${{ github.event.inputs.platform == 'macos-x64' || github.event.inputs.platform == 'all' }}
    runs-on: macos-13  # Intel runner
    steps:
      - uses: actions/checkout@v4

      - name: Build Python bundle
        run: ./scripts/build-python-bundle.sh

      - name: Package bundle
        run: |
          cd src-tauri
          tar -czf python-bundle-macos-x64.tar.gz python/

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: python-bundle-macos-x64
          path: src-tauri/python-bundle-macos-x64.tar.gz
```

---

## Phase 6: Testing Checklist

### 6.1 Bundle Tests

```bash
# Run from project root after building bundle

# 1. Basic import test
./src-tauri/python/bin/python3 -c "import fitz, numpy, pandas, matplotlib; print('OK')"

# 2. PyMuPDF functionality
./src-tauri/python/bin/python3 -c "
import fitz
doc = fitz.open()
page = doc.new_page()
page.insert_text((100, 100), 'Hello from PyMuPDF!')
doc.save('/tmp/test.pdf')
print('PDF created: /tmp/test.pdf')
"

# 3. Data analysis workflow
./src-tauri/python/bin/python3 -c "
import numpy as np
import pandas as pd
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

# Create data
df = pd.DataFrame({
    'x': np.arange(10),
    'y': np.random.randn(10).cumsum()
})

# Plot
plt.figure(figsize=(8, 6))
plt.plot(df['x'], df['y'])
plt.title('Test Plot')
plt.savefig('/tmp/test_plot.png', dpi=100)
print(f'Plot saved, DataFrame shape: {df.shape}')
"

# 4. Integration with pysandbox-rs
cd pysandbox-rs
PYTHON_PATH="../src-tauri/python/bin/python3" cargo test
```

### 6.2 App Bundle Tests

```bash
# After building Tauri app
cargo tauri build

# Verify Python is in bundle
ls -la "target/release/bundle/macos/Rzn.app/Contents/Resources/python/bin/"

# Test from within app context (if possible)
./target/release/bundle/macos/Rzn.app/Contents/Resources/python/bin/python3 -c "
import sys
print(f'Python path: {sys.executable}')
import fitz, numpy, pandas, matplotlib
print('All packages available!')
"
```

---

## Phase 7: Version Matrix

### 7.1 Package Versions (Locked)

| Package | Version | Why This Version |
|---------|---------|------------------|
| Python | 3.11.10 | LTS, best compatibility |
| PyMuPDF | 1.24.14 | Latest stable |
| NumPy | 1.26.4 | Last 1.x, wide compatibility |
| Pandas | 2.2.3 | Latest stable |
| Matplotlib | 3.9.2 | Latest stable |

### 7.2 Future Additions (Download-on-Demand)

| Package | Size | When to Add |
|---------|------|-------------|
| scipy | ~100 MB | Statistical skills requested |
| openpyxl | ~10 MB | Excel formula support |
| scikit-learn | ~50 MB | ML classification skills |
| Pillow | ~15 MB | Image processing skills |

---

## Troubleshooting

### Bundle won't start

```bash
# Check for missing dylibs on macOS
otool -L ./src-tauri/python/bin/python3

# Should only reference system libraries and @executable_path
```

### Package import fails

```bash
# Check if package is installed
./src-tauri/python/bin/python3 -m pip list

# Check for architecture mismatch
file ./src-tauri/python/bin/python3
# Should match your target architecture
```

### Tauri can't find bundled Python

```bash
# Verify tauri.conf.json resources
grep -A5 "resources" src-tauri/tauri.conf.json

# Check built app structure
find "target/release/bundle/macos/Rzn.app" -name "python3"
```

---

## Summary

| Step | Command | Output |
|------|---------|--------|
| 1. Build bundle | `./scripts/build-python-bundle.sh` | `src-tauri/python/` |
| 2. Test bundle | `./src-tauri/python/bin/python3 test.py` | All packages work |
| 3. Build app | `cargo tauri build` | `Rzn.app` with Python |
| 4. Test app | Run app, execute Python skill | Skill runs successfully |

**Expected final bundle size:** ~80-100 MB compressed, ~250-300 MB uncompressed
**Expected app download size:** ~115 MB (app + Python bundle, compressed)
