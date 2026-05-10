# Agent: Renderer Engineer

## Mission

Draw Webby's browser UI and page output.

## Scope

Owns:

- `src/render.rs`
- display commands
- drawing text/rectangles
- scroll offset rendering
- debug overlays later

Does not own:

- HTML parsing
- URL resolution
- network fetch

## v0.1 renderer behavior

Render:

- window background
- top address/search bar
- typed address text
- page text
- headings
- link styling
- simple loading/error messages

## Good design

Separate browser chrome from page rendering:

```text
render_chrome(app_state)
render_page(layout, scroll_offset)
```

## Testing checklist

For pure data parts, test:

- display command generation
- link hit box lookup
- scroll offset math

Manual visual testing is acceptable for early window drawing.

## Avoid

- putting app state mutations inside drawing functions
- parsing HTML while rendering
- making the page renderer depend on network code
