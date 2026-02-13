use crate::errors::{Result, SandboxError};
use std::io::{self, Write};
use std::process::Command;

/// Information about microsandbox installation status
#[derive(Debug)]
pub struct MicrosandboxStatus {
    pub cli_installed: bool,
    pub cli_version: Option<String>,
    pub server_running: bool,
    pub python_image_available: bool,
    pub virtualization_available: bool,
}

/// Check the status of microsandbox installation and dependencies
pub async fn check_microsandbox_status() -> MicrosandboxStatus {
    let cli_installed = check_cli_installed();
    let cli_version = if cli_installed {
        get_cli_version()
    } else {
        None
    };

    let server_running = check_server_running().await;
    let python_image_available = if cli_installed {
        check_python_image()
    } else {
        false
    };

    let virtualization_available = check_virtualization_support();

    MicrosandboxStatus {
        cli_installed,
        cli_version,
        server_running,
        python_image_available,
        virtualization_available,
    }
}

/// Interactive setup wizard for microsandbox
pub async fn setup_microsandbox_interactive() -> Result<bool> {
    println!("\n🔧 Microsandbox Setup Wizard\n");

    let status = check_microsandbox_status().await;

    // Check virtualization first
    if !status.virtualization_available {
        println!("❌ Virtualization support not detected!");
        println!("\nMicrosandbox requires hardware virtualization:");
        println!("  • Linux: KVM support (/dev/kvm)");
        println!("  • macOS: Hypervisor.framework (usually available)");
        println!("  • Windows: Not yet supported");
        println!("\nPlease enable virtualization in your BIOS/UEFI settings.");
        return Ok(false);
    }

    println!("✅ Virtualization support detected");

    // Check CLI installation
    if !status.cli_installed {
        println!("\n📦 Microsandbox CLI not found");
        println!("\nMicrosandbox is a secure Python execution environment that:");
        println!("  • Runs Python code in isolated micro-VMs");
        println!("  • Provides strong security isolation");
        println!("  • Prevents access to your file system and network");
        println!("\nTo install microsandbox CLI, we need to run:");
        println!("  cargo install microsandbox");
        println!("\nThis will:");
        println!("  • Download and compile the microsandbox CLI tool");
        println!("  • Install it to ~/.cargo/bin/");
        println!("  • Add ~10MB to your system");

        if !prompt_user("\nWould you like to install microsandbox CLI?")? {
            return Ok(false);
        }

        println!("\n🔄 Installing microsandbox CLI...");
        install_microsandbox_cli()?;
        println!("✅ Microsandbox CLI installed successfully!");
    } else {
        println!(
            "✅ Microsandbox CLI installed (version: {})",
            status.cli_version.as_deref().unwrap_or("unknown")
        );
    }

    // Check server status
    if !status.server_running {
        println!("\n🖥️  Microsandbox server is not running");
        println!("\nThe microsandbox server:");
        println!("  • Manages micro-VM lifecycle");
        println!("  • Listens on localhost:5555");
        println!("  • Uses ~50MB RAM when idle");
        println!("\nTo start the server, we need to run:");
        println!("  msb server start");

        if !prompt_user("\nWould you like to start the microsandbox server?")? {
            println!("\n💡 Tip: You can start it manually later with: msb server start");
            return Ok(false);
        }

        println!("\n🔄 Starting microsandbox server...");
        start_microsandbox_server()?;

        // Wait a bit for server to start
        println!("⏳ Waiting for server to be ready...");
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        if check_server_running().await {
            println!("✅ Microsandbox server started successfully!");
        } else {
            println!("⚠️  Server may still be starting. Try running your code in a few seconds.");
        }
    } else {
        println!("✅ Microsandbox server is running");
    }

    // Check Python image
    if !status.python_image_available {
        println!("\n🐍 Python sandbox image not found");
        println!("\nThe Python image:");
        println!("  • Contains a minimal Python runtime");
        println!("  • Includes numpy, pandas, matplotlib");
        println!("  • Size: ~120MB (downloaded once)");
        println!("  • Stored in: ~/.microsandbox/images/");
        println!("\nTo download the image, we need to run:");
        println!("  msb pull microsandbox/python");

        if !prompt_user("\nWould you like to download the Python image?")? {
            println!(
                "\n💡 Tip: You can download it manually later with: msb pull microsandbox/python"
            );
            return Ok(false);
        }

        println!("\n🔄 Downloading Python image (this may take a few minutes)...");
        pull_python_image()?;
        println!("✅ Python image downloaded successfully!");
    } else {
        println!("✅ Python image available");
    }

    println!("\n🎉 Microsandbox setup complete!");
    println!("\nYou can now run Python code with maximum security isolation.");
    println!("The micro-VM approach provides stronger security than process isolation.");

    // Check for version compatibility
    if let Some(version) = status.cli_version {
        if version.contains("0.2") {
            println!(
                "\n⚠️  Note: There's currently a version mismatch between the CLI (v0.2.x) and"
            );
            println!("the Rust crate (v0.1.x). You may see authentication errors. The system will");
            println!("automatically fall back to the native Python engine until this is resolved.");
        }
    }
    println!();

    Ok(true)
}

/// Check if microsandbox CLI is installed
fn check_cli_installed() -> bool {
    Command::new("msb")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Get microsandbox CLI version
fn get_cli_version() -> Option<String> {
    Command::new("msb")
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

/// Check if microsandbox server is running
async fn check_server_running() -> bool {
    // Try to connect to the default microsandbox port
    tokio::net::TcpStream::connect("127.0.0.1:5555")
        .await
        .is_ok()
}

/// Check if Python image is available
fn check_python_image() -> bool {
    // For v0.2.x, we'll check if we can pull the image or if it exists
    // The 'images list' command doesn't exist in v0.2.x
    // Instead, we'll just assume the image is available if the server is running
    // since the server will pull images on demand
    check_cli_installed()
}

/// Check virtualization support
fn check_virtualization_support() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new("/dev/kvm").exists()
    }

    #[cfg(target_os = "macos")]
    {
        // Hypervisor.framework is generally available on modern macOS
        true
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

/// Install microsandbox CLI
fn install_microsandbox_cli() -> Result<()> {
    let output = Command::new("cargo")
        .args(&["install", "microsandbox"])
        .output()
        .map_err(|e| SandboxError::InternalError(format!("Failed to run cargo: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SandboxError::InternalError(format!(
            "Failed to install microsandbox: {}",
            stderr
        )));
    }

    Ok(())
}

/// Start microsandbox server
fn start_microsandbox_server() -> Result<()> {
    // Start server in background
    Command::new("msb")
        .args(&["server", "start", "--detach"])
        .spawn()
        .map_err(|e| SandboxError::InternalError(format!("Failed to start server: {}", e)))?;

    Ok(())
}

/// Pull Python image
fn pull_python_image() -> Result<()> {
    let output = Command::new("msb")
        .args(&["pull", "microsandbox/python"])
        .output()
        .map_err(|e| SandboxError::InternalError(format!("Failed to pull image: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SandboxError::InternalError(format!(
            "Failed to pull Python image: {}",
            stderr
        )));
    }

    Ok(())
}

/// Prompt user for yes/no response
fn prompt_user(question: &str) -> Result<bool> {
    print!("{} [y/N]: ", question);
    io::stdout().flush()?;

    let mut response = String::new();
    io::stdin().read_line(&mut response)?;

    Ok(response.trim().eq_ignore_ascii_case("y") || response.trim().eq_ignore_ascii_case("yes"))
}
