# Skill: Add Parser Rule

Use this when adding support for a new HTML tag or parsing behavior.

## Steps

1. Define the tag/behavior.
2. Add a minimal fixture.
3. Update tokenizer/parser only where needed.
4. Update visible text or link extraction if affected.
5. Add tests.

## Required tests

For a new tag, test:

- normal usage
- nested usage if relevant
- unknown/extra attributes if relevant
- visible text output if relevant

## Example target

```text
Support <br> by inserting a line break in visible text and layout input.
```

## Avoid

- full spec implementation unless needed
- parsing CSS/JS here
