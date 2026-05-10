# Codex Master Prompt for Webby

Use this prompt when starting a fresh Codex session.

```text
You are implementing Webby, a from-scratch Rust browser engine and browser shell.

Read these files first:
- AGENTS.md
- docs/ROADMAP.md
- docs/ARCHITECTURE.md
- docs/QUALITY_GATES.md
- docs/modules/*.md relevant to the current milestone

Your job is to implement the next incomplete roadmap milestone thoroughly.

Rules:
- Do not use Chromium, Blink, CEF, Electron, WebKit, Gecko, Servo, or platform WebView for rendering.
- Do not hardcode behavior to pass tests.
- Do not weaken tests to make code pass.
- Prefer serious architecture over quick hacks.
- Keep GUI code separate from engine code.
- Add tests for all pure logic.
- Add CLI debug commands so functionality can be validated without the GUI.
- Update docs if architecture or behavior changes.

Before coding:
1. Identify the next incomplete milestone.
2. Read the module specs for the affected crates.
3. Produce a short implementation plan.
4. Then implement.

During coding:
1. Compile frequently.
2. Run targeted tests first.
3. Then run full validation.

Before finishing, run:
- cargo fmt --all -- --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- relevant webby_cli smoke tests

Final response format:
Summary:
- files changed
- behavior implemented

Validation:
- commands run and results

Definition of done:
- checklist from the roadmap milestone

Limitations:
- honest remaining issues
```
```

## Prompt for one milestone

```text
Implement Milestone <N> from docs/ROADMAP.md.

Read:
- AGENTS.md
- docs/ROADMAP.md
- docs/ARCHITECTURE.md
- docs/QUALITY_GATES.md
- docs/modules/<relevant-module>.md

Implement it completely. No shortcuts. Add tests, CLI validation, docs updates, and run the full validation loop.
```

## Prompt for review-only pass

```text
Do a review-only pass of the current Webby codebase against:
- AGENTS.md
- docs/ROADMAP.md
- docs/QUALITY_GATES.md
- docs/modules/*.md

Do not implement new features. Find shortcuts, hardcoded behavior, missing tests, bad boundaries, panics, incorrect architecture, and incomplete definitions of done.
Return a prioritized fix list.
```
