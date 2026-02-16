# Python Tools demos: charting + public data ideas

This extension is easiest to demo in `policy_id=yolo` with a managed env (`python_env.*`) or with the bundled Data Science variant (`python-tools-ds`).

## Demo goals

- prove Python code executes deterministically (bundled runtime) or flexibly (system + venv)
- generate a chart (PNG) and return it as bytes via `result`
- show outbound network can be policy-limited (simple host allowlist)

## “No network” demos (enterprise-friendly)

### 1) Matplotlib chart from synthetic data

- Works with: `python-tools-ds` (bundled)
- No network required

Example Python snippet (the engine auto-encodes `bytes` results as base64 JSON):

```python
import io
import numpy as np
import matplotlib.pyplot as plt

x = np.linspace(0, 10, 200)
y = np.sin(x)

fig, ax = plt.subplots(figsize=(7, 3))
ax.plot(x, y)
ax.set_title("Sine wave")

buf = io.BytesIO()
fig.savefig(buf, format="png", dpi=160, bbox_inches="tight")
result = buf.getvalue()
```

### 2) Small ML demo (Iris dataset)

- Works with: `python-tools-ds` (bundled; `scikit-learn`)
- No network required

Example:

```python
from sklearn.datasets import load_iris
from sklearn.linear_model import LogisticRegression

X, y = load_iris(return_X_y=True)
model = LogisticRegression(max_iter=200).fit(X, y)
result = {"accuracy": float(model.score(X, y))}
```

## Public data sources that work well for chart demos

These are stable and “enterprise explainable” (non-sensitive):

- GitHub-hosted CSVs (easy allowlist): `raw.githubusercontent.com`
  - Example datasets repo: `https://github.com/datasets/` (many CSVs)
- Our World in Data (large CSVs, time-series): `covid.ourworldindata.org`
- USGS Earthquakes GeoJSON feed (simple JSON): `earthquake.usgs.gov`
- World Bank API (JSON): `api.worldbank.org`
- Open-Meteo (JSON): `api.open-meteo.com`

### Public-data chart demo (USGS earthquakes)

- Host allowlist needed: `earthquake.usgs.gov`
- Works with: `python-tools-ds`

```python
import json, urllib.request
import pandas as pd
import matplotlib.pyplot as plt

url = "https://earthquake.usgs.gov/earthquakes/feed/v1.0/summary/all_day.geojson"
with urllib.request.urlopen(url, timeout=20) as r:
    feed = json.loads(r.read().decode("utf-8"))

rows = []
for f in feed.get("features", []):
    props = f.get("properties") or {}
    rows.append({"mag": props.get("mag"), "place": props.get("place")})

df = pd.DataFrame(rows).dropna()
fig, ax = plt.subplots(figsize=(7,3))
df["mag"].plot(kind="hist", bins=25, ax=ax, title="USGS earthquakes (last day)")
fig.tight_layout()

import io
buf = io.BytesIO()
fig.savefig(buf, format="png", dpi=160)
result = buf.getvalue()
```

## Network allowlist examples

Use `network_allowlist` with exact hosts, wildcard suffix entries, or `*`:

```json
{
  "policy_id": "yolo",
  "network_allowlist": ["raw.githubusercontent.com", "*.usgs.gov"]
}
```

Notes:

- Current enforcement is a runtime socket guard (not a system firewall).
- Enterprises will typically keep `*` disallowed, and approve specific hosts.
