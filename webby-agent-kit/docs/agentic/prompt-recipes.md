# Webby Prompt Recipes

Copy these into your coding agent.

## Create project skeleton

```text
Read AGENTS.md and .agents/context-map.md.
Create the initial Rust project skeleton for Webby.
Add modules for app, URL resolver, networking, DOM, HTML parsing, layout, and rendering.
Do not implement CSS, JavaScript, tabs, or history yet.
Add a simple URL resolver test.
```

## Add URL/search bar resolver

```text
Use .agents/skills/implement-feature.md.
Task: implement address/search input resolution.
Behavior:
- https://example.com stays unchanged
- example.com becomes https://example.com
- rust browser engine becomes https://www.google.com/search?q=rust+browser+engine
Add tests.
```

## Add visible text extraction

```text
Use .agents/agents/parser-engineer.md and .agents/skills/add-parser-rule.md.
Task: parse simple HTML and extract visible text.
Support h1, h2, p, div, span, a, br.
Ignore script and style content.
Add tests.
```

## Add simple layout

```text
Use .agents/agents/layout-engineer.md and .agents/skills/add-layout-rule.md.
Task: create simple vertical layout for headings, paragraphs, and links.
Use fixed viewport width in tests.
Do not add flexbox/grid.
```

## Review a patch

```text
Use .agents/agents/reviewer.md and .agents/skills/review-patch.md.
Review the current patch for scope, correctness, tests, and Webby v0.1 alignment.
```
