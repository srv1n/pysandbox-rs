use pysandbox::{create_sandbox_interactive, engine::ExecutionOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Python Sandbox Interactive Setup Demo ===\n");

    println!("This demo will help you set up microsandbox for enhanced security.");
    println!("Microsandbox runs Python code in isolated micro-VMs.\n");

    // Create sandbox with interactive setup
    let sandbox = create_sandbox_interactive().await?;

    println!("\n📊 Sandbox created! Let's test it:\n");

    // Show available engines
    println!("Available engines:");
    let caps = sandbox.capabilities().await;
    for (i, cap) in caps.iter().enumerate() {
        let marker = if i == 0 { "[PRIMARY]" } else { "[FALLBACK]" };
        println!(
            "  - {} {} (security level: {}/10)",
            cap.name, marker, cap.security_level
        );
    }
    println!();

    // Run a simple test
    let test_code = r#"
import json
import platform
import sys

result = {
    "message": "Hello from Python sandbox!",
    "python_version": sys.version.split()[0],
    "platform": platform.system(),
    "calculation": sum(range(1, 101))  # Sum of 1 to 100
}
"#;

    println!("Running test code...");
    match sandbox
        .execute(
            test_code,
            serde_json::json!({}),
            ExecutionOptions::default(),
        )
        .await
    {
        Ok(result) => {
            println!("✅ Success! Result:");
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Err(e) => {
            println!("❌ Error: {}", e);
        }
    }

    // Show security is active
    println!("\n🔒 Testing security (this should fail):");
    let malicious = "import os\nresult = os.listdir('/')";
    match sandbox
        .execute(
            malicious,
            serde_json::json!({}),
            ExecutionOptions::default(),
        )
        .await
    {
        Ok(_) => println!("⚠️  Unexpected: Security test passed!"),
        Err(e) => println!("✅ Good! Security blocked the operation: {}", e),
    }

    println!("\n✨ Setup complete! You can now use the sandbox in your applications.");

    Ok(())
}
