# Expected Fixture Output Strategy

Fixture integration tests assert deterministic properties directly in Rust:
stable text fragments, URL resolutions, dump equality across repeated runs,
diagnostic ordering, render bytes, and image pixels.

Golden files should only be added here when a future review approves an update
flow that deliberately rewrites expected outputs.
