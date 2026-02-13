use pysandbox::{config::ImportPolicy, create_default_sandbox, engine::ExecutionOptions};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Python Sandbox Simple Demo ===\n");

    // Create sandbox
    let sandbox = create_default_sandbox().await?;

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

    if caps.iter().any(|c| c.name.contains("Microsandbox")) {
        println!("🚀 Microsandbox engine detected! Using VM isolation.\n");
    } else {
        println!("ℹ️  Using native Python engine with guardrails.\n");
    }

    // Example 1: Basic calculation
    println!("Example 1: Basic calculation");
    let code1 = r#"
# Simple calculation without imports
data = [1, 2, 3, 4, 5]
result = {
    'sum': sum(data),
    'mean': sum(data) / len(data),
    'max': max(data),
    'min': min(data),
    'count': len(data)
}
"#;

    match sandbox
        .execute(code1, serde_json::json!({}), ExecutionOptions::default())
        .await
    {
        Ok(result) => println!("Result: {}\n", serde_json::to_string_pretty(&result)?),
        Err(e) => println!("Error: {}\n", e),
    }

    // Example 2: With JSON input/output
    println!("Example 2: JSON data processing");
    let code2 = r#"
import json

# Process input data
products = inputs['products']
total_sales = sum(p['price'] * p['quantity'] for p in products)
avg_price = sum(p['price'] for p in products) / len(products)

result = {
    'total_sales': total_sales,
    'average_price': avg_price,
    'product_count': len(products),
    'most_expensive': max(products, key=lambda p: p['price'])['name']
}
"#;

    let inputs = serde_json::json!({
        "products": [
            {"name": "Widget A", "price": 10.50, "quantity": 100},
            {"name": "Widget B", "price": 25.00, "quantity": 50},
            {"name": "Widget C", "price": 5.99, "quantity": 200}
        ]
    });

    match sandbox
        .execute(code2, inputs, ExecutionOptions::default())
        .await
    {
        Ok(result) => println!("Result: {}\n", serde_json::to_string_pretty(&result)?),
        Err(e) => println!("Error: {}\n", e),
    }

    // Example 3: Security demonstration - blocked operations
    println!("Example 3: Security demonstration");
    let malicious_codes = vec![
        (
            "Trying to access file system",
            "import os\nresult = os.listdir('/')",
        ),
        (
            "Trying to run subprocess",
            "import subprocess\nresult = subprocess.run(['ls'])",
        ),
        (
            "Trying to access network",
            "import socket\ns = socket.socket()",
        ),
        (
            "Trying to write file",
            "result = open('/tmp/test.txt', 'w')",
        ),
    ];

    for (desc, code) in malicious_codes {
        print!("  {} ... ", desc);
        match sandbox
            .execute(code, serde_json::json!({}), ExecutionOptions::default())
            .await
        {
            Ok(_) => println!("⚠️  UNEXPECTED SUCCESS"),
            Err(e) => println!("✅ Blocked: {}", e),
        }
    }

    println!("\n✨ Demo completed!");

    Ok(())
}
