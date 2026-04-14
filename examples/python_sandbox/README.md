# Python Sandbox Quick Start Pack

These quick starts now serve two purposes:

- packaged smoke tests for the base `python-tools` variant
- a curated 5-demo ladder that spans basic Python, business logic, charting,
  ML, and policy-limited public data

If you installed the local CLI bundle, sync this pack into your machine-local workflows directory
with:

```bash
rzn-python-tools workflows sync --force
```

Default sync destination:

- `~/.rzn/python-tools/workflows`

## Included examples

1. `quick_starts/health_probe.json`
2. `quick_starts/list_envs.json`
3. `quick_starts/create_demo_env.json`
4. `quick_starts/run_hello_world.json`
5. `quick_starts/run_in_demo_env.json`
6. `quick_starts/run_basic_stats.json`
7. `quick_starts/run_order_summary.json`
8. `quick_starts/run_ds_plot_synthetic_orders.json`
9. `quick_starts/run_ds_iris_classifier.json`
10. `quick_starts/run_ds_usgs_quakes_chart.json`

## Recommended demo ladder

| Step | File | Best variant |
| --- | --- | --- |
| 1 | `quick_starts/run_basic_stats.json` | `python-tools` |
| 2 | `quick_starts/run_order_summary.json` | `python-tools` |
| 3 | `quick_starts/run_ds_plot_synthetic_orders.json` | `python-tools-ds` |
| 4 | `quick_starts/run_ds_iris_classifier.json` | `python-tools-ds` |
| 5 | `quick_starts/run_ds_usgs_quakes_chart.json` | `python-tools-ds` |

## Notes

- `run_in_demo_env.json` assumes `python_env.create` has already created the `demo` alias.
- Managed env usage is intended for `policy_id = yolo`.
- The DS chart quick starts now return normalized `result.summary` + `result.artifacts[]` payloads.
- The new `run_ds_*` files already target `plugin.python-tools-ds.python`.
- For `python-tools-system`, replace tool IDs with:
  - `mcp:plugin.python-tools-system.python:<tool_name>`
- The full rationale and expected outputs for the 5-demo ladder live in:
  - `docs/PYTHON_TOOLS_DEMOS.md`
