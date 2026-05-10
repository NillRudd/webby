# Module Spec: webby_cli

## Purpose

CLI for debug, validation, and Codex feedback loops.

## Commands

```bash
webby_cli --resolve-input "example.com"
webby_cli --fetch "https://example.com"
webby_cli --dump-dom examples/simple.html
webby_cli --dump-text examples/simple.html
webby_cli --dump-style examples/simple.html
webby_cli --dump-layout examples/simple.html
webby_cli --render-snapshot examples/simple.html --out snapshot.png
```

## Requirements

- CLI is not the main product, but it is essential for automated validation.
- Commands should exit nonzero on real failures.
- Output should be deterministic where possible.
