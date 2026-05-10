# Webby Context Map

This file explains where code should live and what each area owns.

## Main app

```text
src/main.rs
src/app.rs
src/input.rs
```

Owns:

- window setup
- event loop
- keyboard/mouse input
- address/search bar state
- navigation state
- triggering loads

Must not own:

- HTML parsing
- layout algorithms
- drawing primitives beyond calling the renderer

## Networking

```text
src/net.rs
src/url_resolver.rs
```

Owns:

- resolving address bar input
- HTTP/HTTPS fetch
- redirects if needed
- basic error reporting
- relative URL joining for links

Must not own:

- DOM parsing
- rendering

## DOM and parsing

```text
src/dom.rs
src/html.rs
```

Owns:

- DOM node types
- basic HTML tokenization/parsing
- visible text extraction
- link extraction
- ignoring script/style content

v0.1 supports:

- html
- head
- title
- body
- h1/h2/h3
- p
- a href
- div
- span
- br

Unknown tags should usually preserve visible child text.

## Style

```text
src/style.rs
```

Owns:

- default styles for built-in tags
- simple user-agent-style values
- later: simple CSS parsing

v0.1 can avoid full CSS. Start with defaults:

```text
body margin: 16
h1 larger text
h2 medium-large text
p normal text with spacing
a link style
br line break
```

## Layout

```text
src/layout.rs
```

Owns:

- converting DOM/styled content into positioned boxes/text runs
- line wrapping
- block stacking
- scrollable document height

Must not own:

- network loading
- raw drawing backend code

## Rendering

```text
src/render.rs
```

Owns:

- display list commands
- drawing rectangles/text/link styling
- scroll offset
- debug overlays later

Renderer should consume layout/display-list data. It should not parse HTML.

## Tests

```text
tests/
src/* tests modules
examples/
```

Useful fixtures:

```text
examples/simple.html
examples/links.html
examples/text_layout.html
```

