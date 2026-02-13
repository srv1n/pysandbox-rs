use crate::{
    engine::{EngineCapabilities, ExecutionOptions, PythonEngine},
    errors::{Result, SandboxError},
};
use async_trait::async_trait;
use microsandbox::{BaseSandbox, PythonSandbox, StartOptions};

/// Microsandbox-based Python execution engine using official SDK v0.1.2
pub struct MicrosandboxEngine {
    /// Unique name prefix for sandboxes
    name_prefix: String,
}

impl MicrosandboxEngine {
    /// Create a new microsandbox engine
    pub async fn new() -> Result<Self> {
        // Setup authentication from server key
        crate::microsandbox_auth::setup_auth()?;

        Ok(Self {
            name_prefix: format!(
                "pysandbox-{}",
                uuid::Uuid::new_v4().to_string()[..8].to_string()
            ),
        })
    }

    /// Check if microsandbox is available on this system
    pub async fn is_available() -> bool {
        // Check all requirements
        let status = crate::microsandbox_setup::check_microsandbox_status().await;
        status.virtualization_available
            && status.cli_installed
            && status.server_running
            && status.python_image_available
    }

    /// Setup microsandbox interactively if needed
    pub async fn setup_if_needed() -> Result<bool> {
        let status = crate::microsandbox_setup::check_microsandbox_status().await;

        if status.virtualization_available
            && status.cli_installed
            && status.server_running
            && status.python_image_available
        {
            // Everything is ready
            return Ok(true);
        }

        // Run interactive setup
        crate::microsandbox_setup::setup_microsandbox_interactive().await
    }
}

#[async_trait]
impl PythonEngine for MicrosandboxEngine {
    async fn validate(&self, _code: &str, _options: &ExecutionOptions) -> Result<()> {
        // Microsandbox will handle validation during execution
        Ok(())
    }

    async fn execute(
        &mut self,
        code: &str,
        inputs: serde_json::Value,
        options: &ExecutionOptions,
    ) -> Result<serde_json::Value> {
        // Create a unique sandbox name for this execution
        let sandbox_name = format!(
            "{}-{}",
            self.name_prefix,
            uuid::Uuid::new_v4().to_string()[..8].to_string()
        );

        // Create the Python sandbox
        let mut sandbox = PythonSandbox::create(&sandbox_name).await.map_err(|e| {
            SandboxError::MicrosandboxError(format!("Failed to create sandbox: {}", e))
        })?;

        // Configure start options
        let start_options = StartOptions {
            image: Some("microsandbox/python".to_string()),
            memory: options.memory_mb as u32,
            cpus: 1.0,
            timeout: options.timeout.as_secs_f32(),
        };

        // Start the sandbox
        sandbox.start(Some(start_options)).await.map_err(|e| {
            SandboxError::MicrosandboxError(format!("Failed to start sandbox: {}", e))
        })?;

        // Prepare code with input injection and result capture
        let wrapped_code = format!(
            r#"
import json
import sys
import io
import base64

# Inject inputs
inputs = json.loads('''{}''')

# Capture stdout
old_stdout = sys.stdout
sys.stdout = io.StringIO()

# Execute user code
try:
    {}

    # Get stdout content
    output_text = sys.stdout.getvalue()
    sys.stdout = old_stdout

    # Prepare result
    result_data = {{}}

    # Add result if defined
    if 'result' in locals():
        if isinstance(result, bytes):
            # Handle binary data (e.g., matplotlib images)
            result_data['result'] = {{
                'type': 'bytes',
                'data': base64.b64encode(result).decode('utf-8')
            }}
        elif isinstance(result, (dict, list, str, int, float, bool, type(None))):
            result_data['result'] = result
        else:
            result_data['result'] = {{
                'type': str(type(result).__name__),
                'repr': str(result)
            }}

    # Add stdout if any
    if output_text:
        result_data['stdout'] = output_text

    # Output as JSON
    print(json.dumps(result_data))

except Exception as e:
    import traceback
    sys.stdout = old_stdout
    error_data = {{
        'error': str(e),
        'type': type(e).__name__,
        'traceback': traceback.format_exc()
    }}
    print(json.dumps(error_data))
"#,
            serde_json::to_string(&inputs)?.replace("'", "\\'"),
            code.lines()
                .map(|line| format!("    {}", line))
                .collect::<Vec<_>>()
                .join("\n")
        );

        // Execute code with timeout
        let execution = tokio::time::timeout(options.timeout, sandbox.run(&wrapped_code))
            .await
            .map_err(|_| SandboxError::Timeout)?
            .map_err(|e| SandboxError::MicrosandboxError(format!("Execution failed: {}", e)))?;

        // Get output
        let output = execution
            .output()
            .await
            .map_err(|e| SandboxError::MicrosandboxError(format!("Failed to get output: {}", e)))?;

        // Check for errors
        if execution.has_error() {
            let error = execution
                .error()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            // Stop sandbox before returning error
            let _ = sandbox.stop().await;

            return Err(SandboxError::RuntimeError(error));
        }

        // Stop sandbox
        let _ = sandbox.stop().await;

        // Parse the output
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(json_result) => {
                // Check if it has our expected structure
                if let Some(result) = json_result.get("result") {
                    Ok(result.clone())
                } else if let Some(error) = json_result.get("error") {
                    Err(SandboxError::RuntimeError(
                        error.as_str().unwrap_or("Unknown error").to_string(),
                    ))
                } else if json_result.get("stdout").is_some() {
                    // No explicit result but has stdout - return the whole thing
                    Ok(json_result)
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            Err(_) => {
                // Return as string if not JSON
                if output.trim().is_empty() {
                    Ok(serde_json::Value::Null)
                } else {
                    Ok(serde_json::Value::String(output))
                }
            }
        }
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: "Microsandbox VM".to_string(),
            numpy: true,
            matplotlib: true,
            pandas: true,
            max_memory_mb: 4096,
            max_cpu_seconds: 60,
            security_level: 9, // High security via VM isolation
        }
    }

    async fn shutdown(&mut self) -> Result<()> {
        // Sandboxes are ephemeral, nothing to clean up
        Ok(())
    }
}
