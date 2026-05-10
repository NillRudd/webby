# Module Spec: webby_core

## Purpose

Shared low-level types used across Webby.

## Required APIs

- `WebbyError`
- `WebbyResult<T>`
- optional shared geometry primitives only if they are genuinely cross-cutting

## Error variants

At minimum:

- `InvalidInput`
- `Url`
- `Network`
- `Io`
- `Parse`
- `Layout`
- `Render`
- `Unsupported`

Each error should carry enough context to debug the issue.

## Quality requirements

- No GUI dependencies.
- No parser-specific hacks.
- Errors should implement `std::error::Error` through `thiserror` or manual implementation.
