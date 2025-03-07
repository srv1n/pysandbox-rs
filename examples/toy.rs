
use pyo3::prelude::*;
use sandboxed_python::run_sandboxed_code;
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let code = r#"
import numpy as np
result = np.mean(data)
"#;

    let mut inputs = HashMap::new();
    Python::with_gil(|py| {
        let data = vec![1.0, 2.0, 3.0].to_object(py);
        inputs.insert("data".to_string(), data);
    });

    let whitelist = vec!["numpy"];
    let blacklist = vec!["open", "savefig"];

    let result = run_sandboxed_code(code, inputs, &whitelist, &blacklist)?;
    Python::with_gil(|py| {
        println!("Result: {}", result.extract::<f64>(py).unwrap());
    });

    Ok(())
}