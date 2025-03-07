use sandboxed_python::{run_sandboxed_code, SandboxError};
use pyo3::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Python code to generate a Matplotlib plot and return it as bytes
    let code = r#"
import numpy as np
import matplotlib.pyplot as plt
from io import BytesIO

# Generate data
x = np.linspace(0, 2 * np.pi, 100)
y = np.sin(x)

# Create the plot
plt.figure(figsize=(8, 6))
plt.plot(x, y, label='Sine Wave', color='blue')
plt.title('Simple Sine Wave Plot')
plt.xlabel('X')
plt.ylabel('Y')
plt.legend()
plt.grid(True)

# Save to an in-memory buffer instead of a file
buf = BytesIO()
plt.savefig(buf, format='png', dpi=300)
buf.seek(0)
result = buf.read()
buf.close()
plt.close()
"#;

    // Inputs (none needed for this example)
    let inputs = HashMap::new();

    // Whitelist Matplotlib and dependencies
    let whitelist = vec!["numpy", "matplotlib.pyplot", "io"];
    let blacklist = vec!["open"]; // Still block file operations

    // Run the sandboxed code
    let result = run_sandboxed_code(code, inputs, &whitelist, &blacklist)?;

    // Extract the PNG bytes and save to a file in Rust
    Python::with_gil(|py| {
        let bytes: Vec<u8> = result.extract(py)?;
        let mut file = File::create("sine_wave.png")?;
        file.write_all(&bytes)?;
        println!("Plot saved as 'sine_wave.png'");
        Ok::<(), PyErr>(())
    })?;

    Ok(())
}