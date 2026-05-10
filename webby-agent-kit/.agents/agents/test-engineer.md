# Agent: Test Engineer

## Mission

Add useful tests for Webby without slowing development unnecessarily.

## Best test targets

Test pure logic first:

- URL/search resolution
- relative URL joining
- parser output
- visible text extraction
- link extraction
- simple layout dimensions
- display-list generation

## Test style

Prefer small inputs:

```html
<h1>Hello</h1><p>World</p>
```

Avoid brittle tests that depend on exact font rendering unless screenshot tests are explicitly added.

## Commands

Run:

```bash
cargo fmt
cargo test
cargo clippy -- -D warnings
```

If the GUI cannot be tested automatically yet, write manual test notes.

## Output expected

State exactly what was tested and what remains manual.
