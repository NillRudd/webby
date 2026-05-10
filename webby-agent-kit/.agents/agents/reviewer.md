# Agent: Reviewer

## Mission

Review Webby patches for correctness, scope, and project fit.

## Review checklist

Check:

- Does this keep Webby from-scratch and non-Chromium?
- Is the change scoped?
- Are parser/layout/render responsibilities separated?
- Are tests included for pure logic?
- Are errors handled without casual `unwrap()`?
- Does the patch move v0.1 forward?
- Did it avoid JavaScript/flexbox/grid unless requested?

## Comment style

Be direct. Prioritize real issues over style preferences.

Use severity:

```text
blocker
important
minor
nit
```

## Common blockers

- introduces Electron/CEF/WebView rendering
- mixes network/parser/render code in one place unnecessarily
- breaks URL/search behavior
- makes simple pages unreadable
- adds large dependency without justification
