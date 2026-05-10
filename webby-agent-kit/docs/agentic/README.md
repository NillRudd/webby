# Agentic Development for Webby

This folder explains how to use AI coding agents without letting them ruin the project.

Webby is a browser-engine project. Agents should help implement narrow pieces:

- URL resolver
- parser rules
- visible text extraction
- layout rules
- rendering commands
- tests
- reviews

Agents should not silently expand scope into a full browser platform.

Recommended starting prompt:

```text
Read AGENTS.md, .agents/context-map.md, and .agents/workflow.md first.
Use the relevant role and skill files.
Implement only the requested behavior.
```

Useful rule:

```text
One agent task = one coherent behavior change.
```
