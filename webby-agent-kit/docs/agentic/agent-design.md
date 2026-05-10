# Agent Design for Webby

Use specialized agents instead of one vague mega-agent.

## Roles

```text
Architect          keeps scope/architecture sane
Parser Engineer    owns HTML/DOM behavior
Layout Engineer    owns style/layout behavior
Renderer Engineer  owns drawing/display output
Test Engineer      adds focused tests
Reviewer           reviews patches
```

## Why split roles

Browser engines have clear layers. Splitting agents by layer reduces accidental cross-layer hacks.

Good task routing:

```text
Add <br> support              -> Parser Engineer + Layout Engineer if needed
Fix search query URL encoding -> Architect or Parser? Actually networking/url
Paragraph wrapping            -> Layout Engineer
Draw address bar cursor       -> Renderer Engineer
Review current patch          -> Reviewer
```

## Agent boundaries

Every agent should respect:

```text
input -> network -> parse -> style -> layout -> render
```

If a task crosses layers, keep the handoff explicit.
