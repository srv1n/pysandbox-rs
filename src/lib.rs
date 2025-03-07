mod analyzer;
mod errors;
mod executor;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;

pub use errors::SandboxError;

/// Runs Python code in a sandboxed environment.
///
/// # Arguments
/// - `code`: The Python code to execute.
/// - `inputs`: A map of input variables (e.g., {"data": PyObject}).
/// - `whitelist`: List of allowed module names (e.g., ["numpy", "matplotlib.pyplot"]).
/// - `blacklist`: List of disallowed function/method names (e.g., ["open", "savefig"]).
///
/// # Returns
/// - `Ok(PyObject)` containing the execution result.
/// - `Err(SandboxError)` if analysis or execution fails.
///
/// # Example
/// ```rust
/// use sandboxed_python::{run_sandboxed_code, SandboxError};
/// use pyo3::prelude::*;
/// use std::collections::HashMap;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let code = "result = data + 1";
///     let mut inputs = HashMap::new();
///     Python::with_gil(|py| {
///         inputs.insert("data".to_string(), 42.to_object(py));
///     });
///     let whitelist = vec!["numpy"];
///     let blacklist = vec!["open"];
///
///     let result = run_sandboxed_code(code, inputs, &whitelist, &blacklist)?;
///     Python::with_gil(|py| {
///         println!("Result: {}", result.extract::<i32>(py)?);
///     });
///     Ok(())
/// }
/// ```
pub fn run_sandboxed_code(
    code: &str,
    inputs: HashMap<String, PyObject>,
    whitelist: &[&str],
    blacklist: &[&str],
) -> Result<PyObject, SandboxError> {
    Python::with_gil(|py| {
        // Step 1: Static analysis
        analyzer::analyze_code(py, code, whitelist, blacklist)
            .map_err(|e| SandboxError::DisallowedOperation(e.to_string()))?;

        // Step 2: Execute the code
        executor::execute_code(py, code, &inputs, whitelist)
            .map_err(|e| SandboxError::RuntimeError(e.to_string()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::IntoPyDict;

    #[test]
    fn test_basic_execution() {
        let code = "result = x + y";
        let mut inputs = HashMap::new();
        Python::with_gil(|py| {
            inputs.insert("x".to_string(), 3.to_object(py));
            inputs.insert("y".to_string(), 4.to_object(py));
        });
        let whitelist = vec![];
        let blacklist = vec![];
        
        let result = run_sandboxed_code(code, inputs, &whitelist, &blacklist).unwrap();
        Python::with_gil(|py| {
            assert_eq!(result.extract::<i32>(py).unwrap(), 7);
        });
    }
}