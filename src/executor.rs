use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::collections::HashMap;

/// Executes Python code in a restricted environment.
///
/// # Arguments
/// - `py`: The Python GIL token.
/// - `code`: The Python code to execute.
/// - `inputs`: A map of input variables to pass to the code.
/// - `whitelist`: List of allowed module names.
///
/// # Returns
/// - `Ok(PyObject)` containing the result of the execution.
/// - `Err(PyErr)` if execution fails.
pub fn execute_code(
    py: Python,
    code: &str,
    inputs: &HashMap<String, PyObject>,
    whitelist: &[&str],
) -> PyResult<PyObject> {
    // Create a restricted globals dictionary
    let globals = PyDict::new(py);

    // Import only whitelisted modules
    for &module in whitelist {
        let module_obj = PyModule::import(py, module)?;
        globals.set_item(module, module_obj)?;
    }

    // Restrict __builtins__ to exclude dangerous functions
    let builtins = PyModule::import(py, "builtins")?.dict();
    let restricted_builtins = PyDict::new(py);
    for (key, value) in builtins.iter() {
        let key_str = key.extract::<String>()?;
        // Exclude dangerous built-ins
        if !["open", "eval", "exec", "compile"].contains(&key_str.as_str()) {
            restricted_builtins.set_item(key, value)?;
        }
    }
    globals.set_item("__builtins__", restricted_builtins)?;

    // Set input variables
    for (key, value) in inputs {
        globals.set_item(key, value)?;
    }

    // Wrap the code in a function to capture the result
    let indented_code = code
        .lines()
        .map(|line| format!("    {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    let wrapped_code = format!(
        "def _sandboxed_function():\n{}\n    return result\nresult = _sandboxed_function()",
        indented_code
    );

    // Execute the code
    // Assuming wrapped_code is a String defined earlier
    let wrapped_code_cstr = std::ffi::CString::new(wrapped_code).expect("CString::new failed");
    py.run(wrapped_code_cstr.as_c_str(), Some(&globals), None)?;

    // Retrieve the result
    let result = match globals.get_item("result") {
        Ok(Some(v)) => v.into_py(py),
        Ok(None) => py.None(),
        Err(_) => py.None(),
    };
    Ok(result)
}