use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::ffi::CString;

/// Analyzes Python code for disallowed operations based on a whitelist and blacklist.
///
/// # Arguments
/// - `py`: The Python GIL token.
/// - `code`: The Python code to analyze.
/// - `whitelist`: List of allowed module names (e.g., ["numpy", "matplotlib.pyplot"]).
/// - `blacklist`: List of disallowed function/method names (e.g., ["open", "savefig"]).
///
/// # Returns
/// - `Ok(())` if the code is safe.
/// - `Err(PyErr)` if disallowed operations are detected.
pub fn analyze_code(py: Python, code: &str, whitelist: &[&str], blacklist: &[&str]) -> PyResult<()> {
    // Python code for AST analysis
    let analyzer_code = r#"
import ast

def check_code(code, whitelist, blacklist):
    try:
        tree = ast.parse(code)
    except SyntaxError as e:
        raise ValueError(f"Syntax error: {str(e)}")
    
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                if alias.name not in whitelist:
                    raise ValueError(f"Import of '{alias.name}' is not allowed")
        elif isinstance(node, ast.ImportFrom):
            module = node.module or ""
            if module not in whitelist:
                raise ValueError(f"Import from '{module}' is not allowed")
        elif isinstance(node, ast.Call):
            if isinstance(node.func, ast.Name):
                if node.func.id in blacklist:
                    raise ValueError(f"Call to '{node.func.id}' is not allowed")
            elif isinstance(node.func, ast.Attribute):
                if node.func.attr in blacklist:
                    raise ValueError(f"Call to method '{node.func.attr}' is not allowed")
"#;

    // Load the Python analyzer code as a module
    let analyzer_cstr = CString::new(analyzer_code).unwrap();
    let filename_cstr = CString::new("analyzer.py").unwrap();
    let modname_cstr = CString::new("analyzer").unwrap();
    
    let module = PyModule::from_code(
        py, 
        &analyzer_cstr, 
        &filename_cstr, 
        &modname_cstr
    )?;
    let check_code = module.getattr("check_code")?;

    // Prepare arguments
    let kwargs = PyDict::new(py);
    kwargs.set_item("whitelist", whitelist)?;
    kwargs.set_item("blacklist", blacklist)?;

    // Call the analyzer
    check_code.call((code,), Some(&kwargs))?;
    Ok(())
}