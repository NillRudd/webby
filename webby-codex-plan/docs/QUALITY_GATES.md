# Webby Quality Gates

Codex must use this file as the completion checklist for every task.

## Universal gate

A task is not done until all of these are true:

- The implementation is not hardcoded to the tests.
- The implementation handles invalid input gracefully.
- Tests cover the successful case and at least one failure/edge case.
- Public APIs have clear names and module-level docs where useful.
- `cargo fmt --all -- --check` passes.
- `cargo clippy --workspace --all-targets -- -D warnings` passes.
- `cargo test --workspace` passes.
- Relevant CLI smoke tests pass.
- Documentation is updated if behavior or architecture changed.

## No-shortcut checks

Before finishing, inspect the patch for:

- hardcoded URLs except examples/default search engine
- hardcoded pixel dimensions outside configuration/defaults
- parser logic that only handles one fixture
- tests that duplicate implementation internals too closely
- swallowed errors
- `.unwrap()`/`.expect()` in library code
- unnecessary global state
- large functions that should be split
- UI code mixed with parsing/layout internals

## Task report format

Every Codex completion should end with:

```text
Summary:
- ...

Validation:
- cargo fmt --all -- --check
- cargo clippy --workspace --all-targets -- -D warnings
- cargo test --workspace
- ...

Definition of done:
- [x] ...
- [x] ...

Limitations:
- ...
```

## Continuous validation loop

For large milestones, Codex should run validation in layers:

1. `cargo check -p crate_name`
2. crate unit tests
3. `cargo test --workspace`
4. `cargo clippy --workspace --all-targets -- -D warnings`
5. CLI smoke test
6. manual GUI smoke test when relevant

Do not wait until the end of a huge milestone to compile.

## Review gate

After each milestone, run a review pass using `docs/reviews/MILESTONE_REVIEW_TEMPLATE.md`.
