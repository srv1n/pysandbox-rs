# Python Sandbox Quick Start Pack

These quick starts mirror the packaged `system.metadata.yaml` entries for the
`python-tools` variant (`server_name=plugin.python-tools.python`).

## Included examples

1. `quick_starts/health_probe.json`
2. `quick_starts/list_envs.json`
3. `quick_starts/create_demo_env.json`
4. `quick_starts/run_hello_world.json`
5. `quick_starts/run_in_demo_env.json`

## Notes

- `run_in_demo_env.json` assumes `python_env.create` has already created the `demo` alias.
- Managed env usage is intended for `policy_id = yolo`.
- For `python-tools-system` and `python-tools-ds`, replace tool IDs with:
  - `mcp:plugin.python-tools-system.python:<tool_name>`
  - `mcp:plugin.python-tools-ds.python:<tool_name>`
