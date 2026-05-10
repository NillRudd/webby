# Skill: Implement Feature

Use this when adding one coherent Webby feature.

## Steps

1. State the behavior in one sentence.
2. Find the owning module using `.agents/context-map.md`.
3. Add or update the smallest needed data structures.
4. Implement the feature.
5. Add tests for pure logic.
6. Run formatting/tests if possible.
7. Summarize changed files and behavior.

## Keep scope tight

A feature should usually touch one or two areas only.

Good:

```text
Add URL resolver for address/search bar input.
```

Bad:

```text
Add URL resolver, tabs, history, CSS, and downloads.
```

## Done means

- behavior works for the stated examples
- tests cover edge cases
- errors are handled clearly
