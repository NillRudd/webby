# Webby Prompt Templates

Reusable prompts for implementation and review work.

## Implement A Milestone

```text
Read AGENTS.md and docs/ENGINEERING_RULES.md first.

The engineering rules are mandatory. Treat violations as blocking unless
explicitly waived.

Implement Milestone <N> from docs/ROADMAP.md completely.

Follow docs/ARCHITECTURE.md, docs/MODULES.md, docs/AGENT_WORKFLOW.md, and
docs/VALIDATION.md. No shortcuts. Add tests, update docs, and preserve crate
boundaries.
```

## Review A Milestone

```text
Read AGENTS.md and docs/ENGINEERING_RULES.md first.

Review-only pass for Milestone <N>.

Do not implement new features. Check correctness, architecture boundaries,
runtime panic risks, hardcoded behavior, missing tests, stale docs, and whether
the roadmap definition of done is complete.

Run the validation commands from docs/VALIDATION.md.

End with:
1. Blocking issues
2. Important non-blocking issues
3. Missing tests
4. Architecture concerns
5. Engineering rule violations
6. Whether the milestone should be considered complete
```

## Fix Review Findings

```text
Read AGENTS.md and docs/ENGINEERING_RULES.md first.

Fix only the blocking and important non-blocking issues from the previous
review. Do not implement the next milestone. Preserve existing behavior unless
the review identified it as flawed.

Run the validation commands from docs/VALIDATION.md.
```

## Documentation Cleanup

```text
Read AGENTS.md and docs/ENGINEERING_RULES.md first.

Consolidate duplicated docs into the canonical structure. Do not change Rust
behavior. Remove stale generated instruction trees or replace tool-specific
directories with tiny pointers back to AGENTS.md and docs/.
```

## Milestone Review Checklist

- Roadmap tasks implemented.
- Tests added for happy paths, edge cases, and errors.
- CLI/debug validation added when relevant.
- Docs updated.
- No fixture-specific hardcoding.
- No runtime panic markers in runtime code.
- Crate boundaries preserved.
- Validation from `docs/VALIDATION.md` passed.
