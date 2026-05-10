# Webby Agent Workflow

Use this workflow for every coding task.

## 1. Classify the task

Pick one primary area:

```text
app/input
networking/url
html/dom
style
layout
rendering
tests
review/refactor
```

Then use the matching role file from `.agents/agents/`.

## 2. Define the smallest useful result

Before editing, write a one-sentence target:

```text
After this change, Webby can <specific behavior>.
```

Examples:

```text
After this change, Webby resolves plain search text into a Google search URL.
After this change, Webby ignores <script> content during visible text extraction.
After this change, Webby wraps paragraph text inside the page width.
```

## 3. Inspect before changing

Find existing modules, tests, and naming conventions. Do not invent a new architecture if one already exists.

## 4. Implement narrowly

Prefer one feature per patch.

Avoid:

- unrelated formatting
- broad rewrites
- premature abstractions
- adding CSS/JS complexity unless asked

## 5. Add tests

At minimum test pure logic:

- URL resolver
- parser behavior
- visible text extraction
- layout dimensions
- display list shape

Manual window behavior can be described if automated testing is not yet practical.

## 6. Run checks

Run what exists:

```bash
cargo fmt
cargo test
cargo clippy -- -D warnings
```

If a command cannot be run, state that clearly.

## 7. Final response format

Return:

```text
Changed files:
- path
- path

What changed:
- concise bullet

Tests:
- command/result
```

Do not dump unrelated code.
