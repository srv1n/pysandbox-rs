#!/bin/bash
#
# Build Python Bundle for Rzn
#
# This script creates a standalone Python distribution with pre-installed
# packages for bundling with the Tauri app.
#
# Usage:
#   ./scripts/build-python-bundle.sh [--output-dir DIR] [--packages PACKAGES]
#
# Options:
#   --output-dir DIR      Output directory (default: ../src-tauri/python)
#   --packages PACKAGES   Comma-separated list of packages to install
#                         (default: pymupdf,numpy,pandas,matplotlib)
#   --minimal             Only install PyMuPDF (smallest bundle)
#   --full                Install common data science packages (includes scipy + openpyxl)
#   --datascience         Install "demo" data science stack (adds sklearn + pillow)
#   --help                Show this help message
#

set -e

# Configuration
PYTHON_VERSION="3.11.10"
PYTHON_BUILD_DATE="20241016"
DEFAULT_PACKAGES="pymupdf==1.24.14,numpy==1.26.4,pandas==2.2.3,matplotlib==3.9.2"
MINIMAL_PACKAGES="pymupdf==1.24.14"
FULL_PACKAGES="pymupdf==1.24.14,numpy==1.26.4,pandas==2.2.3,matplotlib==3.9.2,scipy==1.14.1,openpyxl==3.1.5"
DATASCIENCE_PACKAGES="pymupdf==1.24.14,numpy==1.26.4,pandas==2.2.3,matplotlib==3.9.2,scipy==1.14.1,scikit-learn==1.5.2,openpyxl==3.1.5,pillow==10.4.0"

# Script location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Defaults
OUTPUT_DIR=""
PACKAGES="$DEFAULT_PACKAGES"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --packages)
            PACKAGES="$2"
            shift 2
            ;;
        --minimal)
            PACKAGES="$MINIMAL_PACKAGES"
            shift
            ;;
        --full)
            PACKAGES="$FULL_PACKAGES"
            shift
            ;;
        --datascience)
            PACKAGES="$DATASCIENCE_PACKAGES"
            shift
            ;;
        --help)
            head -25 "$0" | tail -20
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Set default output directory if not specified
if [ -z "$OUTPUT_DIR" ]; then
    # Check if we're in pysandbox-rs or rznapp
    if [ -d "$PROJECT_ROOT/../src-tauri" ]; then
        OUTPUT_DIR="$PROJECT_ROOT/../src-tauri/python"
    elif [ -d "$PROJECT_ROOT/src-tauri" ]; then
        OUTPUT_DIR="$PROJECT_ROOT/src-tauri/python"
    else
        OUTPUT_DIR="$PROJECT_ROOT/python-bundle"
    fi
fi

# Convert to absolute path (important: must happen before cd to temp dir)
if [[ "$OUTPUT_DIR" != /* ]]; then
    OUTPUT_DIR="$(cd "$(dirname "$OUTPUT_DIR")" 2>/dev/null && pwd)/$(basename "$OUTPUT_DIR")" || OUTPUT_DIR="$(pwd)/$OUTPUT_DIR"
fi

# Detect architecture
ARCH=$(uname -m)
OS=$(uname -s)

case "$OS" in
    Darwin)
        case "$ARCH" in
            arm64|aarch64)
                ARCH_STRING="aarch64-apple-darwin"
                ;;
            x86_64)
                ARCH_STRING="x86_64-apple-darwin"
                ;;
            *)
                echo "❌ Unsupported macOS architecture: $ARCH"
                exit 1
                ;;
        esac
        ;;
    Linux)
        case "$ARCH" in
            x86_64)
                ARCH_STRING="x86_64-unknown-linux-gnu"
                ;;
            aarch64)
                ARCH_STRING="aarch64-unknown-linux-gnu"
                ;;
            *)
                echo "❌ Unsupported Linux architecture: $ARCH"
                exit 1
                ;;
        esac
        ;;
    *)
        echo "❌ Unsupported OS: $OS"
        exit 1
        ;;
esac

echo "🐍 Building Python bundle"
echo "   Platform: $ARCH_STRING"
echo "   Packages: $PACKAGES"
echo "   Output: $OUTPUT_DIR"
echo ""

# Create temp directory
TEMP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_DIR" EXIT
cd "$TEMP_DIR"

# Download python-build-standalone
PYTHON_URL="https://github.com/indygreg/python-build-standalone/releases/download/${PYTHON_BUILD_DATE}/cpython-${PYTHON_VERSION}+${PYTHON_BUILD_DATE}-${ARCH_STRING}-install_only.tar.gz"
echo "📥 Downloading Python from:"
echo "   $PYTHON_URL"
curl -L --progress-bar -o python.tar.gz "$PYTHON_URL"

# Extract
echo "📦 Extracting Python..."
tar -xzf python.tar.gz

# Verify Python works
echo "✓ Python extracted"
./python/bin/python3 --version

# Install packages
echo ""
echo "📚 Installing packages..."
IFS=',' read -ra PACKAGE_ARRAY <<< "$PACKAGES"
for pkg in "${PACKAGE_ARRAY[@]}"; do
    echo "   Installing $pkg..."
    ./python/bin/pip3 install --no-cache-dir "$pkg" --quiet
done

# Verify installations
echo ""
echo "✓ Verifying installations..."
./python/bin/python3 -c "
import sys
print(f'Python {sys.version}')

packages = []
try:
    import fitz
    packages.append(f'PyMuPDF {fitz.version}')
except ImportError:
    pass

try:
    import numpy
    packages.append(f'NumPy {numpy.__version__}')
except ImportError:
    pass

try:
    import pandas
    packages.append(f'Pandas {pandas.__version__}')
except ImportError:
    pass

try:
    import matplotlib
    packages.append(f'Matplotlib {matplotlib.__version__}')
except ImportError:
    pass

try:
    import scipy
    packages.append(f'SciPy {scipy.__version__}')
except ImportError:
    pass

try:
    import openpyxl
    packages.append(f'openpyxl {openpyxl.__version__}')
except ImportError:
    pass

for p in packages:
    print(f'  ✓ {p}')
"

# Cleanup to reduce size
echo ""
echo "🧹 Cleaning up unnecessary files..."
cd python

# Remove test files and documentation
rm -rf share/man 2>/dev/null || true
rm -rf lib/python3.11/test 2>/dev/null || true
rm -rf lib/python3.11/idlelib 2>/dev/null || true
rm -rf lib/python3.11/tkinter 2>/dev/null || true
rm -rf lib/python3.11/turtledemo 2>/dev/null || true
rm -rf lib/python3.11/ensurepip 2>/dev/null || true

# Remove pycache and compiled files
find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
find . -type f -name "*.pyc" -delete 2>/dev/null || true
find . -type f -name "*.pyo" -delete 2>/dev/null || true

# Remove pip and setuptools (not needed at runtime)
rm -rf lib/python3.11/site-packages/pip 2>/dev/null || true
rm -rf lib/python3.11/site-packages/setuptools 2>/dev/null || true
rm -rf lib/python3.11/site-packages/pkg_resources 2>/dev/null || true
rm -rf lib/python3.11/site-packages/*.dist-info/RECORD 2>/dev/null || true

# Remove pip entrypoints (they will be broken once site-packages/pip is removed)
rm -f bin/pip bin/pip3 bin/pip3.11 2>/dev/null || true

cd ..

# Calculate sizes
UNCOMPRESSED_SIZE=$(du -sh python | cut -f1)

# Copy to output
echo ""
echo "📁 Copying to $OUTPUT_DIR..."
rm -rf "$OUTPUT_DIR"
mkdir -p "$(dirname "$OUTPUT_DIR")"
mv python "$OUTPUT_DIR"

# Create compressed archive for reference
ARCHIVE_PATH="${OUTPUT_DIR}.tar.gz"
echo "📦 Creating compressed archive..."
tar -czf "$ARCHIVE_PATH" -C "$(dirname "$OUTPUT_DIR")" "$(basename "$OUTPUT_DIR")"
COMPRESSED_SIZE=$(ls -lh "$ARCHIVE_PATH" | awk '{print $5}')

# Summary
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "✅ Python bundle ready!"
echo ""
echo "📊 Statistics:"
echo "   Uncompressed: $UNCOMPRESSED_SIZE"
echo "   Compressed:   $COMPRESSED_SIZE"
echo ""
echo "📁 Locations:"
echo "   Bundle:  $OUTPUT_DIR"
echo "   Archive: $ARCHIVE_PATH"
echo ""
echo "🧪 Test with:"
echo "   $OUTPUT_DIR/bin/python3 -c \"import fitz; print('OK')\""
echo ""
echo "📝 Next steps:"
echo "   1. Add to tauri.conf.json resources: [\"python/**/*\"]"
echo "   2. Update pysandbox-rs to use bundled Python"
echo "   3. Run 'cargo tauri build' to include in app"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
