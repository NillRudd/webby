# AGENTS.md

Codex entrypoint for **Webby**, a tiny from-scratch browser engine and browser
shell written in Rust.

## Read First

Before implementation or review work, read:

1. `docs/ENGINEERING_RULES.md` - mandatory engineering rules.
2. `docs/ROADMAP.md` - controlling implementation plan.
3. `docs/ARCHITECTURE.md` - controlling crate-boundary document.
4. `docs/VALIDATION.md` - required validation commands.
5. `docs/MODULES.md` - crate responsibilities and public behavior.
6. `docs/AGENT_WORKFLOW.md` - build/review/fix workflow.

Reusable prompts live in `docs/PROMPTS.md`. Architectural decisions live in
`docs/DECISIONS.md` and, when useful, `docs/decisions/`.

## Project Rules

- Build Webby in this repository; do not wrap Chromium, WebKit, Blink, CEF,
  Electron, Gecko, Servo, or platform WebView for page rendering.
- Keep the pipeline directional: input -> URL -> resource -> HTML/DOM ->
  CSS/style -> layout -> display list -> render -> app.
- Keep crate boundaries strict. App and CLI compose existing crates; engine
  crates own their own layers.
- Do not implement out-of-scope browser platform features unless the user
  explicitly requests them.
- Update docs when behavior, architecture, commands, or validation changes.

## Completion Checklist

For normal implementation work, finish with the validation in
`docs/VALIDATION.md` and summarize what changed, what was tested, and any
remaining limitations.
