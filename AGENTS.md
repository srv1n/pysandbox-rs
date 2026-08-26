# Agent Instructions

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **Record remaining work** - Capture follow-up work in the active project tracker when applicable
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update work status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds

## Plugin Release Requirement

If the task includes building or publishing Python Tools plugin artifacts, release completion also
requires backend notification using the backend publish contract documented at:

- `/Users/sarav/Downloads/side/rzn/backend/docs/runbook/plugin_team_release_guide.md`

For plugin release work:

- Building a ZIP alone is not enough.
- Notify the backend through the release registration and catalog publish API flow.
- Publish to local `http://localhost:8082` first, then cloud `https://cloud.rzn.ai`, unless the user explicitly says otherwise.
- Legacy `_PROD` env overrides may still exist, but new automation should prefer `_CLOUD`.
- If local or cloud publish fails at any stage, stop and report exactly what failed.
