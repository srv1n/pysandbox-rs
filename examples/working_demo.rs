use pysandbox::{create_default_sandbox, engine::ExecutionOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Python Sandbox Working Demo ===\n");

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
        println!("ℹ️  Using native Python engine with guardrails.");
        if cfg!(feature = "microsandbox-engine") {
            println!("📝 Note: Microsandbox was configured but may have failed authentication.");
            println!(
                "   This is a known issue with version mismatch (crate v0.1.2 vs server v0.2.x)."
            );
        }
        println!();
    }

    // Example 1: Basic Math and Statistics
    println!("Example 1: Basic Statistics");
    let code = r#"
import json
import math

# Calculate statistics
data = [23, 45, 67, 89, 12, 34, 56, 78, 90, 21]
n = len(data)
mean = sum(data) / n
variance = sum((x - mean) ** 2 for x in data) / n
std_dev = math.sqrt(variance)

result = {
    "count": n,
    "sum": sum(data),
    "mean": round(mean, 2),
    "std_dev": round(std_dev, 2),
    "min": min(data),
    "max": max(data),
    "range": max(data) - min(data)
}
"#;

    match sandbox
        .execute(code, serde_json::json!({}), ExecutionOptions::default())
        .await
    {
        Ok(result) => println!("✅ Result: {}\n", serde_json::to_string_pretty(&result)?),
        Err(e) => println!("❌ Error: {}\n", e),
    }

    // Example 2: Data Processing
    println!("Example 2: Sales Data Processing");
    let code = r#"
# Process sales data
sales = [
    {"product": "A", "price": 10.5, "qty": 100},
    {"product": "B", "price": 25.0, "qty": 50},
    {"product": "C", "price": 5.99, "qty": 200},
    {"product": "D", "price": 15.0, "qty": 75}
]

# Calculate metrics
total_revenue = sum(item["price"] * item["qty"] for item in sales)
total_units = sum(item["qty"] for item in sales)
avg_price = sum(item["price"] for item in sales) / len(sales)
best_seller = max(sales, key=lambda x: x["qty"])["product"]
highest_revenue = max(sales, key=lambda x: x["price"] * x["qty"])

result = {
    "total_revenue": round(total_revenue, 2),
    "total_units": total_units,
    "average_price": round(avg_price, 2),
    "best_seller_by_qty": best_seller,
    "highest_revenue_product": highest_revenue["product"],
    "product_count": len(sales)
}
"#;

    match sandbox
        .execute(code, serde_json::json!({}), ExecutionOptions::default())
        .await
    {
        Ok(result) => println!("✅ Result: {}\n", serde_json::to_string_pretty(&result)?),
        Err(e) => println!("❌ Error: {}\n", e),
    }

    // Example 3: Text Processing
    println!("Example 3: Text Analysis");
    let code = r#"
# Analyze text
text = "The quick brown fox jumps over the lazy dog. The dog was really lazy!"

# Word frequency analysis
words = text.lower().replace(".", "").replace("!", "").split()
word_freq = {}
for word in words:
    word_freq[word] = word_freq.get(word, 0) + 1

# Character frequency
char_freq = {}
for char in text.lower():
    if char.isalpha():
        char_freq[char] = char_freq.get(char, 0) + 1

# Find most common
most_common_word = max(word_freq.items(), key=lambda x: x[1])
most_common_char = max(char_freq.items(), key=lambda x: x[1])

result = {
    "total_words": len(words),
    "unique_words": len(word_freq),
    "most_common_word": {"word": most_common_word[0], "count": most_common_word[1]},
    "most_common_char": {"char": most_common_char[0], "count": most_common_char[1]},
    "word_frequency": dict(sorted(word_freq.items(), key=lambda x: x[1], reverse=True)[:5])
}
"#;

    match sandbox
        .execute(code, serde_json::json!({}), ExecutionOptions::default())
        .await
    {
        Ok(result) => println!("✅ Result: {}\n", serde_json::to_string_pretty(&result)?),
        Err(e) => println!("❌ Error: {}\n", e),
    }

    // Example 4: Date/Time calculations
    println!("Example 4: Date Processing");
    let code = r#"
import datetime
import json

# Calculate business days
start = datetime.date(2024, 1, 1)
end = datetime.date(2024, 1, 31)

# Count weekdays
current = start
weekdays = 0
total_days = 0

while current <= end:
    total_days += 1
    if current.weekday() < 5:  # Monday = 0, Friday = 4
        weekdays += 1
    current += datetime.timedelta(days=1)

# Calculate project timeline
project_start = datetime.date(2024, 1, 15)
project_days = 20  # Working days needed
current = project_start
days_added = 0

while days_added < project_days:
    if current.weekday() < 5:
        days_added += 1
    if days_added < project_days:
        current += datetime.timedelta(days=1)

result = {
    "january_stats": {
        "total_days": total_days,
        "weekdays": weekdays,
        "weekends": total_days - weekdays
    },
    "project": {
        "start": project_start.isoformat(),
        "end": current.isoformat(),
        "duration_days": project_days
    }
}
"#;

    match sandbox
        .execute(code, serde_json::json!({}), ExecutionOptions::default())
        .await
    {
        Ok(result) => println!("✅ Result: {}\n", serde_json::to_string_pretty(&result)?),
        Err(e) => println!("❌ Error: {}\n", e),
    }

    // Example 5: JSON data transformation
    println!("Example 5: JSON Data Transformation");
    let code = r#"
import json

# Transform nested data
employees = [
    {"name": "Alice", "dept": "Engineering", "salary": 95000, "years": 3},
    {"name": "Bob", "dept": "Sales", "salary": 75000, "years": 5},
    {"name": "Charlie", "dept": "Engineering", "salary": 105000, "years": 7},
    {"name": "Diana", "dept": "Marketing", "salary": 85000, "years": 4},
    {"name": "Eve", "dept": "Sales", "salary": 80000, "years": 2}
]

# Group by department
by_dept = {}
for emp in employees:
    dept = emp["dept"]
    if dept not in by_dept:
        by_dept[dept] = {"count": 0, "total_salary": 0, "employees": []}
    by_dept[dept]["count"] += 1
    by_dept[dept]["total_salary"] += emp["salary"]
    by_dept[dept]["employees"].append(emp["name"])

# Calculate department stats
dept_stats = {}
for dept, data in by_dept.items():
    dept_stats[dept] = {
        "employee_count": data["count"],
        "average_salary": round(data["total_salary"] / data["count"], 2),
        "total_cost": data["total_salary"],
        "team": data["employees"]
    }

# Overall stats
total_salary = sum(emp["salary"] for emp in employees)
avg_salary = total_salary / len(employees)
highest_paid = max(employees, key=lambda x: x["salary"])
most_experienced = max(employees, key=lambda x: x["years"])

result = {
    "company_stats": {
        "total_employees": len(employees),
        "total_payroll": total_salary,
        "average_salary": round(avg_salary, 2),
        "departments": len(dept_stats)
    },
    "department_breakdown": dept_stats,
    "notable_employees": {
        "highest_paid": {"name": highest_paid["name"], "salary": highest_paid["salary"]},
        "most_experienced": {"name": most_experienced["name"], "years": most_experienced["years"]}
    }
}
"#;

    match sandbox
        .execute(code, serde_json::json!({}), ExecutionOptions::default())
        .await
    {
        Ok(result) => println!("✅ Result: {}\n", serde_json::to_string_pretty(&result)?),
        Err(e) => println!("❌ Error: {}\n", e),
    }

    // Security check
    println!("Security Verification:");
    let tests = vec![
        ("File access", "open('/etc/passwd', 'r')"),
        ("Import os", "import os"),
        ("Network", "import socket"),
        ("Subprocess", "import subprocess"),
    ];

    for (name, code) in tests {
        print!("  Testing {} ... ", name);
        match sandbox
            .execute(code, serde_json::json!({}), ExecutionOptions::default())
            .await
        {
            Ok(_) => println!("⚠️  UNEXPECTED SUCCESS"),
            Err(_) => println!("✅ Blocked"),
        }
    }

    println!("\n✨ Demo completed successfully!");
    Ok(())
}
