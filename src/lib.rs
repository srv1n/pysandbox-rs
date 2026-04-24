pub mod config;
pub mod engine;
pub mod errors;
pub mod native;
pub mod policy;
pub mod reporting;
pub mod sandbox_builder;
pub mod sandboxed;

#[cfg(feature = "microsandbox-engine")]
pub mod microsandbox_engine;

#[cfg(feature = "microsandbox-engine")]
pub mod microsandbox_setup;

#[cfg(feature = "microsandbox-engine")]
pub mod microsandbox_auth;

pub use config::{ExecutionMode, ImportPolicy, ResourceLimits, SecurityProfile};
pub use engine::{EngineCapabilities, ExecutionOptions, PythonEngine};
pub use errors::{Result, SandboxError};
pub use reporting::{
    classify_sandbox_failure, FlowFailureErrorCode, FlowFailurePlatform, FlowFailureReportDraft,
    FlowFailureStage, SandboxFailureClassification,
};

use std::sync::Arc;
use tokio::sync::RwLock;

/// Main sandbox manager that handles multiple execution engines
pub struct PythonSandbox {
    engines: Vec<Arc<RwLock<Box<dyn PythonEngine>>>>,
    primary_engine: usize,
}

impl PythonSandbox {
    /// Create a new sandbox with the specified engines
    pub fn new(engines: Vec<Box<dyn PythonEngine>>) -> Self {
        Self {
            engines: engines
                .into_iter()
                .map(|e| Arc::new(RwLock::new(e)))
                .collect(),
            primary_engine: 0,
        }
    }

    /// Execute Python code using the primary engine with fallback support
    pub async fn execute(
        &self,
        code: &str,
        inputs: serde_json::Value,
        options: ExecutionOptions,
    ) -> Result<serde_json::Value> {
        let primary = &self.engines[self.primary_engine];

        match primary
            .write()
            .await
            .execute(code, inputs.clone(), &options)
            .await
        {
            Ok(result) => Ok(result),
            Err(e) if self.engines.len() > 1 => {
                tracing::warn!("Primary engine failed: {}, trying fallback", e);
                // Try fallback engines
                for (idx, engine) in self.engines.iter().enumerate() {
                    if idx != self.primary_engine {
                        match engine
                            .write()
                            .await
                            .execute(code, inputs.clone(), &options)
                            .await
                        {
                            Ok(result) => return Ok(result),
                            Err(e) => tracing::warn!("Fallback engine {} failed: {}", idx, e),
                        }
                    }
                }
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    /// Execute Python code and emit a sanitized failure report draft if all engines fail.
    ///
    /// The callback receives only the canonical `FlowFailureReportDraft`; raw code,
    /// stdout, stderr, stack traces, paths, env vars, and logs are not copied into it.
    pub async fn execute_with_failure_report_hook<F>(
        &self,
        code: &str,
        inputs: serde_json::Value,
        options: ExecutionOptions,
        on_failure: F,
    ) -> Result<serde_json::Value>
    where
        F: Fn(FlowFailureReportDraft) + Send + Sync,
    {
        let result = self.execute(code, inputs, options).await;
        if let Err(error) = &result {
            on_failure(FlowFailureReportDraft::from_sandbox_error(error, None));
        }
        result
    }

    /// Get capabilities of all engines
    pub async fn capabilities(&self) -> Vec<EngineCapabilities> {
        let mut caps = Vec::new();
        for engine in &self.engines {
            caps.push(engine.read().await.capabilities());
        }
        caps
    }
}

// Re-export sandbox creation functions
pub use sandbox_builder::{
    create_bundled_sandbox, create_default_sandbox, create_sandbox_interactive,
    create_sandbox_with_options, SandboxOptions,
};

// Re-export native engine for direct use
pub use native::NativePythonEngine;

// Re-export sandboxed engine and types
pub use sandboxed::{
    IsolatedWorkspace, SandboxConfig, SandboxedExecutionBuilder, SandboxedExecutionResult,
    SandboxedPythonEngine,
};

// Re-export policy system
pub use policy::{
    // Enterprise
    EnterprisePolicy,
    ExecutionEnvironment,
    FilesystemPolicy,
    ImportPolicyType,
    // Primitives
    NetworkPolicy,
    PolicyManager,
    ProcessPolicy,
    ResourceLimitsPolicy,
    // Policy
    SandboxPolicy,
    SandboxPolicyBuilder,
};

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    struct FailingEngine;

    #[async_trait]
    impl PythonEngine for FailingEngine {
        async fn validate(&self, _code: &str, _options: &ExecutionOptions) -> Result<()> {
            Ok(())
        }

        async fn execute(
            &mut self,
            _code: &str,
            _inputs: serde_json::Value,
            _options: &ExecutionOptions,
        ) -> Result<serde_json::Value> {
            Err(SandboxError::RuntimeError(
                "Traceback with /private/path and secret".to_string(),
            ))
        }

        fn capabilities(&self) -> EngineCapabilities {
            EngineCapabilities {
                name: "failing".to_string(),
                numpy: false,
                matplotlib: false,
                pandas: false,
                max_memory_mb: 1,
                max_cpu_seconds: 1,
                security_level: 0,
            }
        }

        async fn shutdown(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn host_hook_emits_sanitized_draft_only() {
        let sandbox = PythonSandbox::new(vec![Box::new(FailingEngine)]);
        let emitted = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&emitted);

        let result = sandbox
            .execute_with_failure_report_hook(
                "print('secret code')",
                serde_json::json!({ "prompt": "secret prompt" }),
                ExecutionOptions::default(),
                move |draft| captured.lock().unwrap().push(draft),
            )
            .await;

        assert!(result.is_err());
        let drafts = emitted.lock().unwrap();
        assert_eq!(drafts.len(), 1);
        let value = serde_json::to_value(&drafts[0]).unwrap();
        assert_eq!(value["failed_stage"], "execute");
        assert_eq!(value["error"], "runtime_error");

        let serialized = value.to_string();
        assert!(!serialized.contains("secret code"));
        assert!(!serialized.contains("secret prompt"));
        assert!(!serialized.contains("Traceback"));
        assert!(!serialized.contains("/private/path"));
        assert!(!serialized.contains("secret"));
    }
}
