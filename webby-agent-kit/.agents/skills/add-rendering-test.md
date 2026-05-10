# Skill: Add Rendering Test

Use this when adding or stabilizing page rendering behavior.

## Preferred order

1. Test display-list generation first.
2. Test hit boxes/scroll math if relevant.
3. Add screenshot tests only when rendering backend is stable.

## Display-list tests

Useful assertions:

- contains text command for heading
- contains text command for paragraph
- contains link metadata for anchors
- commands respect scroll/page offsets

## Manual visual test note

For early GUI work, document manual steps:

```text
Manual: run `cargo run -- examples/simple.html`, type `example.com`, press Enter.
Expected: top bar remains visible, page area shows fetched text.
```
