---
name: implement-milestone
description: Implement one Webby roadmap milestone thoroughly with tests, docs, validation, and definition-of-done checks.
---

# Implement Milestone Skill

Use this when implementing a full milestone from `docs/ROADMAP.md`.

## Process

1. Read `AGENTS.md`.
2. Read `docs/ROADMAP.md`.
3. Read `docs/ARCHITECTURE.md`.
4. Read `docs/QUALITY_GATES.md`.
5. Read relevant `docs/modules/*.md` files.
6. Identify exact milestone tasks and definition of done.
7. Implement in small compileable chunks.
8. Add tests as code is added.
9. Add or update CLI validation when relevant.
10. Update docs if APIs or architecture changed.
11. Run validation.
12. Produce final report in the required format.

## Anti-shortcut checklist

- No fixture-only parser logic.
- No hardcoded URLs except configurable defaults.
- No rendering through WebView/browser engines.
- No skipped tests.
- No `.unwrap()` in library paths unless impossible and justified.
