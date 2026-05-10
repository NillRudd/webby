# Skill: Debug Failure

Use this when Webby fails, crashes, renders wrong output, or cannot fetch/parse a page.

## Steps

1. Reproduce the failure with the smallest input.
2. Identify the failing layer:

```text
input/url/network/html/dom/style/layout/render
```

3. Add a regression test if the failure is pure logic.
4. Fix the smallest responsible layer.
5. Re-run the failing case.
6. Summarize cause and fix.

## Useful debug commands

Add or use commands like:

```bash
webby dump-url "rust browser engine"
webby dump-html https://example.com
webby dump-dom examples/simple.html
webby dump-layout examples/simple.html
```

## Avoid

- fixing symptoms in renderer when parser is wrong
- broad rewrites during debugging
- adding special cases for one website unless explicitly intended
