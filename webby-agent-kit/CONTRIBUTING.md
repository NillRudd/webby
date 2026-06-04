# Contributing

Webby is deliberately small and boundary-conscious. Before changing code, read:

- [`AGENTS.md`](AGENTS.md)
- [`docs/ENGINEERING_RULES.md`](docs/ENGINEERING_RULES.md)
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- [`docs/ROADMAP.md`](docs/ROADMAP.md)

Keep changes scoped to one subsystem. Parser behavior belongs in parser crates,
style application in `webby_style`, layout in `webby_layout`, rendering in
`webby_render`, and shell interaction in `webby_app`. Do not add engine behavior
only for a fixture.

Run the validation loop before submitting a change:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
rg 'unwrap\(|expect\(|panic!|todo!|unimplemented!' crates
```

Add focused tests for each behavior change and update the compatibility or
limitations docs when support changes.
