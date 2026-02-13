# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`pysandbox-rs` is a Rust library for executing Python code in a sandboxed environment. It provides security through multiple execution engines with different isolation levels:
- **Native Python Engine**: Process isolation with resource limits (Security Level: 5/10)
- **Microsandbox Engine**: VM-based isolation using libkrun (Security Level: 9/10)

## Development Commands

### Build
```bash
# Build without microsandbox support
cargo build

# Build with microsandbox support
cargo build --features microsandbox-engine

# Release build
cargo build --release
```

### Test
```bash
# Run all tests
cargo test

# Run with microsandbox feature
cargo test --features microsandbox-engine

# Run specific test with output
cargo test test_basic_execution -- --nocapture
```

### Lint and Format
```bash
# Run clippy for linting
cargo clippy --all-features

# Format code
cargo fmt

# Fix common issues
cargo fix --allow-dirty
```

### Examples
```bash
# Run working demo (basic data analysis)
cargo run --example working_demo

# Run with microsandbox support
cargo run --example working_demo --features microsandbox-engine

# Run interactive microsandbox setup
cargo run --example interactive_setup --features microsandbox-engine

# Test microsandbox authentication
cargo run --example test_microsandbox --features microsandbox-engine
```

### Documentation
```bash
# Generate and open documentation
cargo doc --open --all-features
```

## Architecture

The library uses a trait-based architecture with multiple execution engines:

### Core Components

1. **engine.rs**: Defines the `PythonEngine` trait
   - Common interface for all execution engines
   - Async execution with timeout support
   - Capability reporting

2. **native.rs**: Native Python engine implementation
   - Subprocess-based isolation
   - Resource limits via rlimit (Unix) and Job Objects (Windows)
   - Import blacklisting for security
   - Stdout/stderr capture

3. **microsandbox_engine.rs**: Microsandbox VM engine (optional)
   - Uses microsandbox crate for VM isolation
   - Highest security level
   - Requires external server and setup

4. **lib.rs**: Main sandbox manager
   - Handles multiple engines with fallback
   - Primary engine with automatic failover
   - Unified API regardless of engine used

### Supporting Modules

- **config.rs**: Configuration types (ResourceLimits, ImportPolicy)
- **errors.rs**: Error types for different failure modes
- **sandbox_builder.rs**: Convenience functions for creating sandboxes
- **microsandbox_setup.rs**: Interactive setup wizard for microsandbox
- **microsandbox_auth.rs**: Authentication handling for microsandbox

## Security Model

### Native Python Engine
- Process isolation with subprocess
- Resource limits (memory, CPU, processes)
- Import blacklisting (blocks os, subprocess, socket, etc.)
- Timeout protection
- No file system access

### Microsandbox Engine
- Full VM isolation using libkrun
- Complete kernel separation
- No network access
- No filesystem access
- Strongest security guarantees

## Known Issues

1. Import restrictions are handled differently on macOS vs Linux
2. Matplotlib image generation requires special handling
3. **Microsandbox Compatibility**: The library now includes a custom v0.2.x-compatible client that works with JWT authentication. Microsandbox is fully functional when the server is running.

## Testing Security

When testing security features, verify:
1. File access is blocked
2. Network access is blocked
3. Subprocess creation is blocked
4. Dangerous imports are blocked (os, subprocess, socket, etc.)
5. Resource limits are enforced

## Platform Notes

### macOS
- RLIMIT_AS not supported, uses RLIMIT_DATA instead
- Hypervisor.framework used for microsandbox

### Linux
- Full rlimit support
- Requires /dev/kvm for microsandbox

### Windows
- Uses Job Objects for resource limits
- Microsandbox not yet supported