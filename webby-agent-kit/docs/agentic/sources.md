# Sources Behind This Agent Setup

This project uses a practical subset of common agentic-coding patterns.

## Repo-local instructions

Use a repo-level `AGENTS.md` so coding agents know project rules, scope, commands, and style.

Useful for Webby because the agent must avoid turning the project into a Chromium/WebView wrapper.

## Skills

Reusable skill files work well for repeated procedures:

- implement one feature
- debug one failure
- review one patch
- add parser/layout/render tests

Useful for Webby because the same routines repeat many times while building the engine.

## Specialized agents

Split agents by responsibility:

- parser
- layout
- renderer
- tests
- reviewer
- architect

Useful for Webby because browser engines are naturally layered.

## Workflow agents

Use deterministic workflows for common work:

```text
inspect -> plan -> implement -> test -> review
```

Use bounded repair loops for failures:

```text
run test -> inspect failure -> patch -> run test again
```

## Tool/context separation

Agents should not hide tool behavior. Network fetch, parser output, layout output, and rendering output should be separately inspectable.

Useful for Webby because debugging browser engines is much easier when each pipeline stage can be dumped independently.
