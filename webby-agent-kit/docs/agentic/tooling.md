# Tooling Notes

## Required local checks

```bash
cargo fmt
cargo test
```

Recommended when stable:

```bash
cargo clippy -- -D warnings
```

## Useful future dev commands

Consider adding these CLI/debug commands:

```bash
webby dump-url "rust browser engine"
webby dump-html https://example.com
webby dump-text https://example.com
webby dump-dom examples/simple.html
webby dump-layout examples/simple.html
```

These commands make agentic debugging much easier because each pipeline stage can be inspected separately.

## Good fixtures

```text
examples/simple.html
examples/links.html
examples/script_ignored.html
examples/text_wrapping.html
```
