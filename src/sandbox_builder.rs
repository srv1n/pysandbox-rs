use crate::{
    config::ResourceLimits, native::NativePythonEngine, PythonEngine, PythonSandbox, Result,
    SandboxError,
};
use std::path::PathBuf;

/// Options for creating a sandbox
pub struct SandboxOptions {
    /// Whether to prompt for microsandbox setup if not available
    pub interactive_setup: bool,
    /// Whether to prefer microsandbox over native engine
    pub prefer_microsandbox: bool,
    /// Custom Python path (for bundled Python)
    pub python_path: Option<PathBuf>,
    /// Custom resource limits
    pub limits: Option<ResourceLimits>,
}

impl Default for SandboxOptions {
    fn default() -> Self {
        Self {
            interactive_setup: false,
            prefer_microsandbox: true,
            python_path: None,
            limits: None,
        }
    }
}

impl SandboxOptions {
    /// Create options for bundled Python
    pub fn bundled(python_path: PathBuf) -> Self {
        Self {
            interactive_setup: false,
            prefer_microsandbox: false, // Use bundled Python directly
            python_path: Some(python_path),
            limits: None,
        }
    }

    /// Set custom resource limits
    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = Some(limits);
        self
    }
}

/// Create sandbox with custom options
pub async fn create_sandbox_with_options(options: SandboxOptions) -> Result<PythonSandbox> {
    let mut engines: Vec<Box<dyn PythonEngine>> = vec![];

    // Try to add microsandbox if available and feature enabled
    #[cfg(feature = "microsandbox-engine")]
    {
        if options.prefer_microsandbox && options.python_path.is_none() {
            let mut should_use_microsandbox = false;

            // Check if microsandbox is ready
            if crate::microsandbox_engine::MicrosandboxEngine::is_available().await {
                should_use_microsandbox = true;
            } else if options.interactive_setup {
                // Try interactive setup
                println!("\n🔍 Microsandbox is not fully configured.");
                println!("Microsandbox provides stronger security through VM isolation.");

                if prompt_setup_microsandbox()? {
                    match crate::microsandbox_engine::MicrosandboxEngine::setup_if_needed().await {
                        Ok(true) => {
                            println!("✅ Microsandbox is now ready to use!");
                            should_use_microsandbox = true;
                        }
                        Ok(false) => {
                            println!(
                                "ℹ️  Microsandbox setup cancelled. Using native Python engine."
                            );
                        }
                        Err(e) => {
                            println!("⚠️  Microsandbox setup failed: {}", e);
                            println!("ℹ️  Falling back to native Python engine.");
                        }
                    }
                }
            }

            if should_use_microsandbox {
                match crate::microsandbox_engine::MicrosandboxEngine::new().await {
                    Ok(engine) => engines.push(Box::new(engine)),
                    Err(e) => {
                        tracing::warn!("Failed to create microsandbox engine: {}", e);
                    }
                }
            }
        }
    }

    // Create native engine (with bundled Python path if provided)
    let native_engine = match (&options.python_path, &options.limits) {
        (Some(path), Some(limits)) => {
            NativePythonEngine::with_python_path_and_limits(path.clone(), limits.clone())?
        }
        (Some(path), None) => NativePythonEngine::with_python_path(path.clone())?,
        (None, Some(limits)) => NativePythonEngine::with_limits(limits.clone())?,
        (None, None) => NativePythonEngine::new()?,
    };
    engines.push(Box::new(native_engine));

    if engines.is_empty() {
        return Err(SandboxError::NoEngineAvailable);
    }

    Ok(PythonSandbox::new(engines))
}

/// Create sandbox with bundled Python
///
/// Use this when shipping a bundled Python distribution with your Tauri app.
///
/// # Example
/// ```rust,ignore
/// let python_path = app.path_resolver()
///     .resolve_resource("python/bin/python3")
///     .expect("Bundled Python not found");
/// let sandbox = create_bundled_sandbox(python_path).await?;
/// ```
pub async fn create_bundled_sandbox(python_path: PathBuf) -> Result<PythonSandbox> {
    create_sandbox_with_options(SandboxOptions::bundled(python_path)).await
}

/// Create default sandbox (non-interactive)
pub async fn create_default_sandbox() -> Result<PythonSandbox> {
    create_sandbox_with_options(SandboxOptions::default()).await
}

/// Create sandbox with interactive setup if needed
pub async fn create_sandbox_interactive() -> Result<PythonSandbox> {
    create_sandbox_with_options(SandboxOptions {
        interactive_setup: true,
        ..Default::default()
    })
    .await
}

/// Prompt user if they want to set up microsandbox
fn prompt_setup_microsandbox() -> Result<bool> {
    use std::io::{self, Write};

    print!("Would you like to set up microsandbox for enhanced security? [y/N]: ");
    io::stdout().flush()?;

    let mut response = String::new();
    io::stdin().read_line(&mut response)?;

    Ok(response.trim().eq_ignore_ascii_case("y") || response.trim().eq_ignore_ascii_case("yes"))
}
