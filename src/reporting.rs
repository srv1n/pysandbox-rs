use crate::errors::SandboxError;
use serde::{Deserialize, Serialize};

pub const FLOW_FAILURE_SCHEMA_VERSION: u32 = 1;
pub const FLOW_FAILURE_SUBMISSION_MODE: &str = "host_auto";
pub const FLOW_FAILURE_SOURCE: &str = "rzn-python-sandbox";
pub const FLOW_FAILURE_PRODUCT: &str = "rzn-python-sandbox";
pub const FLOW_FAILURE_KIND: &str = "python_sandbox";
pub const FLOW_FAILURE_SURFACE: &str = "python";
pub const FLOW_FAILURE_FLOW: &str = "python/execute-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowFailurePlatform {
    Macos,
    Windows,
    Linux,
    Unknown,
}

impl FlowFailurePlatform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Unknown
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowFailureStage {
    PrepareEnv,
    ResolveRuntime,
    InstallDeps,
    StageFiles,
    Execute,
    EnforceLimits,
    CollectResult,
    Cleanup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowFailureErrorCode {
    RuntimeError,
    Timeout,
    DependencyInstallFailed,
    DependencyResolutionFailed,
    MemoryLimitExceeded,
    CpuLimitExceeded,
    DiskLimitExceeded,
    PermissionDenied,
    RuntimeMissing,
    ResultCollectionFailed,
    UnknownFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandboxFailureClassification {
    pub failed_stage: FlowFailureStage,
    pub error: FlowFailureErrorCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowFailureReportDraft {
    pub schema_version: u32,
    pub submission_mode: String,
    pub source: String,
    pub product: String,
    pub flow_kind: String,
    pub surface: String,
    pub flow: String,
    pub flow_version: String,
    pub failed_stage: FlowFailureStage,
    pub error: FlowFailureErrorCode,
    pub app_version: String,
    pub platform: FlowFailurePlatform,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl FlowFailureReportDraft {
    pub fn new(
        classification: SandboxFailureClassification,
        flow_version: impl Into<String>,
        app_version: impl Into<String>,
        note: Option<String>,
    ) -> Self {
        Self {
            schema_version: FLOW_FAILURE_SCHEMA_VERSION,
            submission_mode: FLOW_FAILURE_SUBMISSION_MODE.to_string(),
            source: FLOW_FAILURE_SOURCE.to_string(),
            product: FLOW_FAILURE_PRODUCT.to_string(),
            flow_kind: FLOW_FAILURE_KIND.to_string(),
            surface: FLOW_FAILURE_SURFACE.to_string(),
            flow: FLOW_FAILURE_FLOW.to_string(),
            flow_version: flow_version.into(),
            failed_stage: classification.failed_stage,
            error: classification.error,
            app_version: app_version.into(),
            platform: FlowFailurePlatform::current(),
            note,
        }
    }

    pub fn from_sandbox_error(error: &SandboxError, note: Option<String>) -> Self {
        Self::new(
            classify_sandbox_failure(error),
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_VERSION"),
            note,
        )
    }

    pub fn from_parts(
        failed_stage: FlowFailureStage,
        error: FlowFailureErrorCode,
        note: Option<String>,
    ) -> Self {
        Self::new(
            SandboxFailureClassification {
                failed_stage,
                error,
            },
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_VERSION"),
            note,
        )
    }
}

pub fn classify_sandbox_failure(error: &SandboxError) -> SandboxFailureClassification {
    use FlowFailureErrorCode as Code;
    use FlowFailureStage as Stage;

    let (failed_stage, error) = match error {
        SandboxError::PythonNotFound | SandboxError::NoEngineAvailable => {
            (Stage::ResolveRuntime, Code::RuntimeMissing)
        }
        SandboxError::Timeout => (Stage::Execute, Code::Timeout),
        SandboxError::MemoryLimitExceeded => (Stage::EnforceLimits, Code::MemoryLimitExceeded),
        SandboxError::ProcessLimitExceeded => (Stage::EnforceLimits, Code::CpuLimitExceeded),
        SandboxError::ImportNotAllowed(_)
        | SandboxError::DisallowedOperation(_)
        | SandboxError::SecurityViolation(_) => (Stage::Execute, Code::PermissionDenied),
        SandboxError::SyntaxError(_)
        | SandboxError::RuntimeError(_)
        | SandboxError::ProcessExitCode(_)
        | SandboxError::ProcessKilled => (Stage::Execute, Code::RuntimeError),
        SandboxError::JsonError(_) => (Stage::CollectResult, Code::ResultCollectionFailed),
        SandboxError::IoError(_) => (Stage::Execute, Code::UnknownFailure),
        SandboxError::InternalError(_) | SandboxError::MicrosandboxError(_) => {
            (Stage::Execute, Code::UnknownFailure)
        }
    };

    SandboxFailureClassification {
        failed_stage,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const FORBIDDEN_KEYS: &[&str] = &[
        "code",
        "prompt",
        "stdout",
        "stderr",
        "stack",
        "stack_trace",
        "traceback",
        "exception",
        "exception_message",
        "message",
        "file_path",
        "path",
        "file_name",
        "dependency_list",
        "dependencies",
        "env",
        "env_vars",
        "generated_files",
        "artifacts",
        "run_id",
        "trace_id",
        "logs",
    ];

    #[test]
    fn snapshot_minimal_payload() {
        let version = env!("CARGO_PKG_VERSION");
        let draft = FlowFailureReportDraft::new(
            SandboxFailureClassification {
                failed_stage: FlowFailureStage::Execute,
                error: FlowFailureErrorCode::RuntimeError,
            },
            version,
            version,
            None,
        );

        let mut expected = json!({
            "schema_version": 1,
            "submission_mode": "host_auto",
            "source": "rzn-python-sandbox",
            "product": "rzn-python-sandbox",
            "flow_kind": "python_sandbox",
            "surface": "python",
            "flow": "python/execute-v1",
            "flow_version": version,
            "failed_stage": "execute",
            "error": "runtime_error",
            "app_version": version,
            "platform": "unknown"
        });
        expected["platform"] = serde_json::to_value(FlowFailurePlatform::current()).unwrap();

        assert_eq!(serde_json::to_value(draft).unwrap(), expected);
    }

    #[test]
    fn forbidden_keys_are_absent() {
        let raw = SandboxError::RuntimeError(
            "Traceback (most recent call last):\n  File \"/Users/me/private.py\", line 1\nsecret"
                .to_string(),
        );
        let value =
            serde_json::to_value(FlowFailureReportDraft::from_sandbox_error(&raw, None)).unwrap();
        let obj = value.as_object().unwrap();

        for key in FORBIDDEN_KEYS {
            assert!(!obj.contains_key(*key), "forbidden key present: {key}");
        }
    }

    #[test]
    fn raw_stack_trace_maps_to_stable_error_only() {
        let raw = SandboxError::RuntimeError(
            "Traceback (most recent call last):\n  File \"/tmp/user/foo.py\", line 2\nValueError: token=abc"
                .to_string(),
        );
        let value =
            serde_json::to_value(FlowFailureReportDraft::from_sandbox_error(&raw, None)).unwrap();

        assert_eq!(value["failed_stage"], Value::String("execute".to_string()));
        assert_eq!(value["error"], Value::String("runtime_error".to_string()));
        assert!(!value.to_string().contains("Traceback"));
        assert!(!value.to_string().contains("/tmp/user/foo.py"));
        assert!(!value.to_string().contains("token=abc"));
    }

    #[test]
    fn timeout_memory_and_dependency_failures_classify() {
        assert_eq!(
            classify_sandbox_failure(&SandboxError::Timeout),
            SandboxFailureClassification {
                failed_stage: FlowFailureStage::Execute,
                error: FlowFailureErrorCode::Timeout,
            }
        );
        assert_eq!(
            classify_sandbox_failure(&SandboxError::MemoryLimitExceeded),
            SandboxFailureClassification {
                failed_stage: FlowFailureStage::EnforceLimits,
                error: FlowFailureErrorCode::MemoryLimitExceeded,
            }
        );

        let dependency = FlowFailureReportDraft::from_parts(
            FlowFailureStage::InstallDeps,
            FlowFailureErrorCode::DependencyInstallFailed,
            None,
        );
        assert_eq!(dependency.failed_stage, FlowFailureStage::InstallDeps);
        assert_eq!(
            dependency.error,
            FlowFailureErrorCode::DependencyInstallFailed
        );
    }
}
