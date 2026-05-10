# Module Spec: webby_render

## Purpose

Convert layout output into display commands and pixels.

## Required design

Use a display list between layout and renderer.

```rust
pub enum DisplayCommand {
    FillRect { rect: Rect, color: Color },
    DrawText { text: String, x: f32, y: f32, size: f32, color: Color },
    StrokeRect { rect: Rect, color: Color, width: f32 },
    Line { from: Point, to: Point, color: Color, width: f32 },
}
```

## Requirements

- Renderer must not parse HTML.
- Renderer must not own navigation state.
- Display list must be testable without opening a window.

## Tests

- simple layout creates expected display commands
- link text creates underline/visual distinction
- scroll offset affects rendered command positions or viewport clipping
