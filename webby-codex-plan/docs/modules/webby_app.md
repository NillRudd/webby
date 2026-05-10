# Module Spec: webby_app

## Purpose

Native browser shell.

## v0.1 UI

- top address/search bar
- page viewport below
- loading/error state
- scroll support
- click links

## Requirements

- Keep address bar state separate from loaded document state.
- Pipeline calls should be explicit.
- Do not put parser/layout code in app event handlers.
- Network failures should show an error page.

## Manual smoke test

1. Run `cargo run -p webby_app`.
2. Type `example.com`.
3. Press Enter.
4. Verify page text appears.
5. Type `rust browser engine`.
6. Verify resolver creates a search URL and attempts navigation.
