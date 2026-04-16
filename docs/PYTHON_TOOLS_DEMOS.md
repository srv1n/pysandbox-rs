# Python Tools Demo Suite

This file defines a recommended demo ladder for `rzn-python-sandbox` and the Python
Tools extension. The goal is not to show random Python snippets; it is to show
why this sandbox is useful:

- basic deterministic execution
- structured JSON input and output
- artifact generation via normalized `summary` + `artifacts[]` payloads
- scientific Python and ML
- outbound network under an explicit allowlist

These demos prove the worker contract and the worker-level security story. They do not claim cross-platform OS sandbox parity.

## Recommended progression

```text
basic compute
  -> business data wrangling
  -> chart artifact generation
  -> ML inference / analysis
  -> public data fetch with policy controls
```

## Demo suite at a glance

| Demo | Level | Best variant | Quick start file | Best user-facing proof |
| --- | --- | --- | --- | --- |
| Basic statistics | basic | `python-tools` | `examples/python_sandbox/quick_starts/run_basic_stats.json` | deterministic JSON result |
| Order summary | basic-to-mid | `python-tools` | `examples/python_sandbox/quick_starts/run_order_summary.json` | `inputs` to structured business metrics |
| Synthetic chart | mid | `python-tools-ds` | `examples/python_sandbox/quick_starts/run_ds_plot_synthetic_orders.json` | normalized PNG artifact bundle |
| Iris classifier | advanced | `python-tools-ds` | `examples/python_sandbox/quick_starts/run_ds_iris_classifier.json` | real ML libraries working in sandbox |
| USGS public-data chart | advanced | `python-tools-ds` | `examples/python_sandbox/quick_starts/run_ds_usgs_quakes_chart.json` | live fetch + allowlist + plot |

## Placement guidance

| Where | What belongs there | Notes |
| --- | --- | --- |
| `docs/PYTHON_TOOLS_DEMOS.md` | demo narrative, positioning, expected outputs | this file should stay as the canonical walkthrough |
| `examples/python_sandbox/quick_starts/` | one-click Tool Bench payloads | best place for the full demo ladder |
| `examples/simple_demo.rs` | basic stats + security checks | keep this Rust example lightweight and dependency-free |
| future `examples/data_science_demo.rs` | charting, ML, public-data demos | better than overloading `examples/working_demo.rs` with unrelated mini-examples |

## Demo 1: Basic Statistics

- Goal: prove the `python_sandbox` contract with the smallest possible useful example
- Variant: `python-tools`
- Quick start: `examples/python_sandbox/quick_starts/run_basic_stats.json`
- Policy: `balanced`

### Python snippet

```python
numbers = inputs["numbers"]
mean = sum(numbers) / len(numbers)

result = {
    "count": len(numbers),
    "sum": sum(numbers),
    "mean": round(mean, 2),
    "min": min(numbers),
    "max": max(numbers),
}
```

### Expected output

This one should be exact and deterministic:

```json
{
  "count": 6,
  "sum": 99,
  "mean": 16.5,
  "min": 7,
  "max": 25
}
```

### Why this demo works

- teaches `inputs` and `result` without extra noise
- safe to show first in a live demo
- makes later examples feel like a natural extension, not a different product

## Demo 2: Order Summary

- Goal: show business-style data wrangling instead of toy arithmetic
- Variant: `python-tools`
- Quick start: `examples/python_sandbox/quick_starts/run_order_summary.json`
- Policy: `balanced`

### Python snippet

```python
orders = inputs["orders"]
revenue_by_region = {}
customer_totals = {}

for order in orders:
    region = order["region"]
    revenue_by_region[region] = round(
        revenue_by_region.get(region, 0.0) + order["amount"], 2
    )

    customer = order["customer"]
    customer_totals[customer] = round(
        customer_totals.get(customer, 0.0) + order["amount"], 2
    )

top_customer = max(customer_totals.items(), key=lambda item: item[1])
largest_order = max(orders, key=lambda order: order["amount"])

result = {
    "order_count": len(orders),
    "total_revenue": round(sum(order["amount"] for order in orders), 2),
    "average_order_value": round(
        sum(order["amount"] for order in orders) / len(orders), 2
    ),
    "top_customer": {"name": top_customer[0], "revenue": top_customer[1]},
    "largest_order_id": largest_order["order_id"],
    "revenue_by_region": revenue_by_region,
}
```

### Expected output

This one should also be exact:

```json
{
  "order_count": 4,
  "total_revenue": 4120.0,
  "average_order_value": 1030.0,
  "top_customer": {
    "name": "Acme",
    "revenue": 1840.0
  },
  "largest_order_id": "A103",
  "revenue_by_region": {
    "NA": 1840.0,
    "EU": 850.0,
    "APAC": 1430.0
  }
}
```

### Why this demo works

- shows data transformation, grouping, and ranking
- looks like a real internal-tool or agent workflow
- still runs on the base plugin without the data-science bundle

## Demo 3: Synthetic Orders Chart

- Goal: show artifact generation and binary output
- Variant: `python-tools-ds`
- Quick start: `examples/python_sandbox/quick_starts/run_ds_plot_synthetic_orders.json`
- Policy: `data_science`

### Python snippet

```python
import base64
import io
import numpy as np
import matplotlib.pyplot as plt

days = np.arange(7)
orders = np.array([120, 128, 133, 129, 141, 150, 160])

fig, ax = plt.subplots(figsize=(7, 3))
ax.plot(days, orders, marker="o", linewidth=2)
ax.set_title("Synthetic daily orders")
ax.set_xlabel("Day")
ax.set_ylabel("Orders")
ax.grid(alpha=0.3)

buf = io.BytesIO()
fig.savefig(buf, format="png", dpi=160, bbox_inches="tight")
png_bytes = buf.getvalue()
print(f"rendered {len(orders)} points")
result = {
    "summary": {"points": int(len(orders))},
    "artifacts": [
        {
            "type": "image",
            "mime_type": "image/png",
            "title": "Synthetic daily orders",
            "data_base64": base64.b64encode(png_bytes).decode("utf-8"),
        }
    ],
}
```

### Expected output

- `stdout` contains `rendered 7 points`
- `result.summary.points` is `7`
- `result.artifacts[0].mime_type` is `image/png`
- decoding `result.artifacts[0].data_base64` yields a valid PNG image

### Why this demo works

- proves the sandbox can return artifacts, not just JSON
- deterministic and network-free
- is visually more compelling than a plain JSON payload

## Demo 4: Iris Classifier

- Goal: show a recognizable ML workflow with bundled libraries
- Variant: `python-tools-ds`
- Quick start: `examples/python_sandbox/quick_starts/run_ds_iris_classifier.json`
- Policy: `data_science`

### Python snippet

```python
from sklearn.datasets import load_iris
from sklearn.linear_model import LogisticRegression

iris = load_iris()
X, y = iris.data, iris.target

model = LogisticRegression(max_iter=200).fit(X, y)
result = {
    "samples": int(X.shape[0]),
    "features": int(X.shape[1]),
    "classes": iris.target_names.tolist(),
    "training_accuracy": round(float(model.score(X, y)), 3),
}
```

### Expected output

- `samples` should be `150`
- `features` should be `4`
- `classes` should be `["setosa", "versicolor", "virginica"]`
- `training_accuracy` should be roughly `0.97` and generally remain `>= 0.95`

The exact accuracy can vary slightly across `scikit-learn` versions, so the
class names and dataset dimensions are the real invariants to demo.

### Why this demo works

- clearly communicates "this is real Python, not a toy interpreter"
- proves the bundled DS variant is valuable
- gives an advanced demo without requiring outbound network

## Demo 5: USGS Earthquake Chart With Allowlist

- Goal: combine public data, policy controls, and chart output in one story
- Variant: `python-tools-ds`
- Quick start: `examples/python_sandbox/quick_starts/run_ds_usgs_quakes_chart.json`
- Policy: `data_science`
- Network allowlist: `["*.usgs.gov"]`

### Python snippet

```python
import base64
import io
import json
import urllib.request

import matplotlib.pyplot as plt
import pandas as pd

url = "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/all_day.geojson"
with urllib.request.urlopen(url, timeout=20) as response:
    feed = json.loads(response.read().decode("utf-8"))

rows = []
for feature in feed.get("features", []):
    props = feature.get("properties") or {}
    if props.get("mag") is not None:
        rows.append({"mag": props["mag"], "place": props.get("place", "unknown")})

df = pd.DataFrame(rows)
print(f"fetched {len(df)} earthquakes")

fig, ax = plt.subplots(figsize=(7, 3))
df["mag"].plot(kind="hist", bins=20, ax=ax, title="USGS earthquakes (last day)")
ax.set_xlabel("Magnitude")
ax.grid(alpha=0.3)

buf = io.BytesIO()
fig.savefig(buf, format="png", dpi=160, bbox_inches="tight")
png_bytes = buf.getvalue()
result = {
    "summary": {
        "earthquake_count": int(len(df)),
        "histogram_bins": 20,
    },
    "artifacts": [
        {
            "type": "image",
            "mime_type": "image/png",
            "title": "USGS earthquakes (last day)",
            "data_base64": base64.b64encode(png_bytes).decode("utf-8"),
        }
    ],
}
```

### Expected output

- `stdout` contains `fetched N earthquakes`
- `N` should be greater than `0`, but will vary over time
- `result.summary.earthquake_count` tracks the fetched row count
- `result.artifacts[0].mime_type` is `image/png`
- removing the allowlist or changing it to a non-matching host should fail

### Why this demo works

- demonstrates the product's security story, not just its Python story
- uses public, explainable, non-sensitive data
- is the strongest "complex" demo in the set

## Demo order for live presentations

Use this order in most product demos:

1. Basic statistics
2. Order summary
3. Synthetic orders chart
4. Iris classifier
5. USGS earthquake chart

This sequence steadily increases complexity without making the audience switch
mental models between each step.

## Network allowlist reminder

`network_allowlist` supports:

- exact hosts: `api.openai.com`
- wildcard suffixes: `*.usgs.gov`
- allow-all: `*`

Current enforcement is a runtime socket guard, not a system firewall. For
enterprise demos, prefer a narrow allowlist and call it out explicitly.
