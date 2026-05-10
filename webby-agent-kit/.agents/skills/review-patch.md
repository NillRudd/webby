# Skill: Review Patch

Use this when reviewing current changes.

## Checklist

- Webby remains from-scratch and non-Chromium.
- Change is scoped to the requested behavior.
- Module ownership is respected.
- Pure logic has tests.
- No unnecessary large dependencies.
- No casual `unwrap()` in fallible runtime paths.
- Error messages are understandable.
- Formatting passes.

## Output format

```text
Verdict: approve / needs changes

Issues:
- severity: description

Suggested fixes:
- concise action
```
