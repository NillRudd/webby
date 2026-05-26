# Webby Agent Workflow

This is the canonical workflow for AI-assisted work on Webby.

## Build Or Fix

1. Read `AGENTS.md` and the mandatory docs it links.
2. Identify the owning crate from `docs/ARCHITECTURE.md` and
   `docs/MODULES.md`.
3. State the desired behavior in one sentence.
4. Inspect existing code and tests before editing.
5. Make the smallest coherent change.
6. Add happy-path, edge-case, and error tests where relevant.
7. Run targeted validation, then the standard commands in `docs/VALIDATION.md`.
8. Update docs when behavior, commands, or architecture changes.

## Review Only

Use a review stance: findings first, ordered by severity, with file and line
references. Check:

- roadmap definition of done
- crate boundaries
- runtime panic risks
- malformed input and tiny viewport behavior
- fixture-specific shortcuts
- missing tests
- stale docs

Do not implement features during a review-only pass.

## Debug A Failure

1. Reproduce the smallest failing input.
2. Identify the layer: URL, net, HTML/DOM, CSS, style, layout, render, app, CLI,
   or state.
3. Add a regression test when the failure is pure logic.
4. Fix the responsible layer, not a downstream symptom.
5. Re-run the failing command and the relevant validation set.

## Refactor Safely

1. Identify behavior that must stay unchanged.
2. Add tests first if behavior is untested.
3. Move or rename code in small steps.
4. Keep public APIs stable unless the task asks for an API change.
5. Avoid mixing behavior changes with mechanical cleanup.

## What Not To Do

- Do not add JavaScript, full browser-platform features, or browser-engine
  wrappers unless explicitly requested by the roadmap or the user.
- Do not move parser/style/layout/render algorithms into app or CLI.
- Do not weaken tests to make code pass.
- Do not create new agent instruction trees. Add concise canonical docs instead.
