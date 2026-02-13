# Microsandbox Setup Guide

## Overview

Microsandbox provides the highest level of security for Python execution by running code in isolated micro-VMs. This guide explains how to set up and use microsandbox with pysandbox-rs.

## Security Levels

| Engine | Security Level | Isolation Method | Performance |
|--------|---------------|------------------|-------------|
| Native Python | 5/10 | Process + Resource Limits | Fast (~10ms) |
| Microsandbox | 9/10 | Micro-VM | Moderate (~200ms) |

## Prerequisites

### Hardware Requirements
- **CPU**: x86_64 with virtualization support (Intel VT-x or AMD-V)
- **RAM**: 4GB minimum (8GB recommended)
- **Disk**: 200MB for microsandbox + images

### Software Requirements
- **Linux**: KVM support (check with `ls /dev/kvm`)
- **macOS**: macOS 10.15+ (uses Hypervisor.framework)
- **Windows**: Not yet supported

## Installation Methods

### Method 1: Interactive Setup (Recommended)

Run the interactive setup example:

```bash
cargo run --example interactive_setup --features microsandbox-engine
```

This will:
1. Check your system for virtualization support
2. Guide you through installing the microsandbox CLI
3. Start the microsandbox server
4. Download the Python image
5. Verify everything is working

### Method 2: Manual Setup

#### Step 1: Install Microsandbox CLI

```bash
cargo install microsandbox
```

This installs:
- `msb` command-line tool to `~/.cargo/bin/`
- Size: ~10MB

#### Step 2: Start the Server

```bash
# Start in foreground (for testing)
msb server start

# Or start in background
msb server start --detach
```

The server:
- Listens on `localhost:5555`
- Manages micro-VM lifecycle
- Uses ~50MB RAM when idle

#### Step 3: Download Python Image

```bash
msb pull microsandbox/python
```

This downloads:
- Minimal Python runtime with numpy, pandas, matplotlib
- Size: ~120MB
- Stored in: `~/.microsandbox/images/`

#### Step 4: Verify Installation

```bash
# List available images
msb images list

# Test Python execution
msb run microsandbox/python -c "print('Hello from microsandbox!')"
```

## Using Microsandbox in Your Code

### Basic Usage

```rust
use pysandbox::{create_default_sandbox, ExecutionOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // This automatically uses microsandbox if available
    let sandbox = create_default_sandbox().await?;
    
    let code = "result = sum([1, 2, 3, 4, 5])";
    let result = sandbox.execute(
        code, 
        serde_json::json!({}),
        ExecutionOptions::default()
    ).await?;
    
    println!("Result: {}", result);
    Ok(())
}
```

### With Interactive Setup

```rust
use pysandbox::{create_sandbox_interactive, ExecutionOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // This will prompt to set up microsandbox if needed
    let sandbox = create_sandbox_interactive().await?;
    
    // ... use sandbox ...
    Ok(())
}
```

### Force Specific Engine

```rust
use pysandbox::{create_sandbox_with_options, SandboxOptions};

// Force microsandbox only (no fallback)
let sandbox = create_sandbox_with_options(SandboxOptions {
    prefer_microsandbox: true,
    interactive_setup: false,
}).await?;
```

## Authentication

### How It Works

Microsandbox v0.2.x uses JWT-based authentication:

1. **Token Generation**: The library automatically generates a JWT token using `msb server keygen`
2. **Token Usage**: The token is passed as a Bearer token in the Authorization header
3. **Token Lifetime**: Tokens are valid for 24 hours by default

### Manual Token Generation

If needed, you can manually generate tokens:

```bash
# Generate a token valid for 7 days
msb server keygen --expire 7d

# Generate a token for a specific namespace
msb server keygen --namespace production
```

## Troubleshooting

### "Virtualization not supported"
- **Linux**: Enable VT-x/AMD-V in BIOS, ensure KVM module is loaded
- **macOS**: Should work on all modern Macs

### "Server not running"
```bash
# Check if server is running
msb server status

# Start server
msb server start --detach

# Check logs
msb server logs
```

### "Image not found"
In microsandbox v0.2.x, images are automatically pulled when needed. If you see image-related errors:

```bash
# Manually pull the Python image
msb pull microsandbox/python

# The server will also pull images on-demand during sandbox creation
```

### "Connection refused"
- Ensure firewall allows localhost:5555
- Check server is running: `ps aux | grep msb`

## Security Benefits

Microsandbox provides:
- **Complete isolation**: Each execution in separate VM
- **No shared kernel**: VM has its own kernel
- **Resource limits**: CPU, memory enforced by hypervisor
- **No network access**: VMs have no network by default
- **No filesystem access**: Can't read host files

## Performance Considerations

### Startup Time
- First run: ~500ms (VM creation)
- Subsequent runs: ~200ms (warm VM)

### Memory Usage
- Server: ~50MB
- Per VM: ~100MB
- Shared between executions

### Optimization Tips
1. Reuse sandbox instances when possible
2. Use native engine for trusted code
3. Batch operations to amortize startup cost

## Uninstalling

To completely remove microsandbox:

```bash
# Stop server
msb server stop

# Remove images
rm -rf ~/.microsandbox

# Uninstall CLI
cargo uninstall microsandbox
```

## FAQ

**Q: Is microsandbox required?**
A: No, pysandbox-rs falls back to native Python with security restrictions.

**Q: Can I use custom Python packages?**
A: Currently limited to pre-installed packages in the image.

**Q: Does it work in containers?**
A: Yes, if the container has access to /dev/kvm (Linux) or runs on macOS.

**Q: What's the security difference?**
A: Native engine prevents obvious attacks; microsandbox prevents even sophisticated exploits.

## Additional Resources

- [Microsandbox GitHub](https://github.com/microsandbox/microsandbox)
- [Microsandbox Documentation](https://docs.rs/microsandbox)
- [libkrun](https://github.com/slp/libkrun) (underlying technology)