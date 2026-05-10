# Milestone Review Template

Use this after completing each milestone.

## Milestone

Name/number:

## Definition of done check

- [ ] All roadmap tasks implemented.
- [ ] Tests added.
- [ ] CLI validation added if relevant.
- [ ] Docs updated.
- [ ] No hardcoded fixture-specific behavior.
- [ ] No hidden panics in library code.
- [ ] Architecture boundaries preserved.

## Validation commands

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Additional commands:

```bash
# add milestone-specific commands here
```

## Review notes

### Correctness

### Architecture

### Test coverage

### Error handling

### Performance

### User-facing behavior

## Required fixes before next milestone

1.
2.
3.
