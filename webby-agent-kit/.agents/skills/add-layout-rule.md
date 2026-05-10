# Skill: Add Layout Rule

Use this when changing how Webby positions text/boxes.

## Steps

1. Define expected layout in plain language.
2. Add a small layout test input.
3. Update style defaults if needed.
4. Update layout calculations.
5. Ensure renderer does not need DOM-specific hacks.

## Good layout tests

- compare x/y/width/height numerically
- avoid pixel-perfect font rasterization
- use fixed viewport width

## Avoid

- implementing flexbox/grid early
- mixing layout and rendering
