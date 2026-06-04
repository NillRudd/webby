# Expected Fixture Output Strategy

Fixture integration tests assert deterministic properties directly in Rust:
stable text fragments, URL resolutions, dump equality across repeated runs,
diagnostic ordering, render bytes, and image pixels.

`corpus-compatibility-report.txt` is a reviewed golden compatibility report.
Update it deliberately after inspecting a behavior change; do not regenerate it
blindly. Other golden files should only be added through the same review flow.

`html-optional-end-tags.dom.txt` is a reviewed forgiving-parser DOM dump for
common omitted HTML end tags.
