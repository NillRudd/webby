# Webby Known Limitations

Webby is a from-scratch browser prototype. It intentionally supports a narrow,
deterministic browser subset.

## Implemented Scope

- HTTP, HTTPS, and file resource loading.
- HTML parsing for common static documents and scoped form/image metadata.
- Scoped CSS parsing, selector matching, cascade, and default styles.
- Block, inline, flex subset, grid subset, simple table, image, and
  form-control layout.
- Software display-list rendering with deterministic text and image output.
- Native shell with tabs, navigation, forms, cache, bookmarks, history, and
  basic cookies.
- Minimal classic JavaScript execution with deterministic `console.log`
  diagnostics, a placeholder `window`, a small mutating `document` API, and
  basic click/input/submit DOM events. Loader-backed pipelines support inline
  scripts, external classic scripts with missing or `text/javascript` type, and
  small JavaScript module graphs with static local/relative imports.
- Small JavaScript browser APIs for `location.href`, `location.assign`,
  `history.back`, `history.forward`, `setTimeout`, and `clearTimeout`.
- JavaScript `fetch`/`XMLHttpRequest` v0.1 for static string `GET` requests
  loaded through Webby's normal resource pipeline, with same-origin/CORS checks.
- Basic JavaScript Web Storage APIs: persistent per-origin `localStorage` and
  non-persistent per-tab `sessionStorage`.
- Inline SVG basics for rectangles, lines, circles, and basic `viewBox`
  scaling, plus a deterministic canvas 2D subset for `fillRect`,
  `strokeRect`, and `clearRect`.
- Data URL image loading, favicon metadata extraction, preload/preconnect
  no-op diagnostics, common MIME sniffing, and deterministic audio/video
  placeholders with video poster rendering.
- Basic iframe loading through the normal page pipeline, with clipped rendered
  child surfaces and independent iframe link navigation.

## Not Implemented

- Full browser JavaScript APIs.
- JavaScript intervals and wall-clock asynchronous scheduling.
- Full SVG, complex paths, gradients, filters, text-on-path, transparent
  canvas compositing, and complete canvas 2D APIs. Current `clearRect`
  records a deterministic clear command that restores the page background in
  Webby's software renderer.
- No audio/video playback, seeking, buffering, codecs, or media controls beyond
  static placeholders. GIF/WebP decoding and favicon loading are not yet
  implemented.
- Dynamic fetch URL expressions, real Promise scheduling, request bodies,
  credentials modes, preflight requests, and full Fetch/XHR semantics.
- Full DOM event coverage, including broad keyboard event support.
- Spec-complete DOM APIs. Current JavaScript DOM bindings cover common
  traversal, selector, class/dataset/style, and layout-geometry reads, but not
  the full browser DOM surface.
- `querySelector` selector forms outside Webby's supported CSS subset, such as
  sibling combinators and unsupported pseudo-classes.
- HTTP cache-control semantics.
- Full cookie expiry date parsing and full RFC cookie behavior.
- TLS/certificate UI.
- Full CSS layout: complete Grid, complete flexbox, floats, transforms, full
  media queries, full pseudo-classes, pseudo-elements, keyframe animations, and web
  fonts.
- Full HTML forms: multipart upload, constraint validation, file inputs,
  rich select popups, and full textarea behavior.
- Accessibility tree, printing, downloads, or developer network panel.
- Full iframe platform behavior, including nested iframe loading beyond one
  level, sandbox enforcement, iframe form integration, `srcdoc`, permissions
  policy, referrer policy, and cross-frame scripting.

## Simplifications

- Text shaping is deterministic and simple; complex scripts, bidi, ligatures,
  emoji, and platform font matching are not complete.
- Large-document tests prove bounded deterministic behavior, not browser-grade
  performance.
- Cookie `Expires` marks a cookie as persistent, but the date is not interpreted
  yet. `Max-Age=0` or negative values delete matching cookies.
- The in-memory resource cache keys by requested URL string and ignores HTTP
  cache headers.
- CSS Grid support is intentionally small: no `repeat()`, named lines,
  subgrid, dense auto-placement, `span` placement syntax, or advanced
  intrinsic track sizing.
- Media query support is viewport-only and deterministic: `screen`/`all` with
  width features and optional orientation. Media queries do not support media
  lists, range syntax, device features, or container queries.
- Pseudo-class support is scoped to common interaction and child-position
  selectors. `:hover`, `:focus`, and `:active` are driven by app-supplied DOM
  node ids; `:checked`, `:disabled`, `:first-child`, `:last-child`, and simple
  `:nth-child(n)` use DOM state. Pseudo-elements, sibling-sensitive selectors,
  and broad form-state pseudo-classes are not implemented.
- Transition support is page/display-list scoped and deterministic. Webby
  parses simple transition longhands and shorthand for `color`,
  `background-color`, `left`, and `top`, then interpolates matching display
  command colors and geometry during app-driven rerenders. It does not
  implement keyframes, transforms, opacity, per-element animation timelines, or
  real wall-clock animation scheduling.
- External script ordering is deterministic rather than concurrent: blocking
  classic scripts run first in document order, `defer` scripts run after those,
  and `async` scripts run last in document order. The direct `render_html(...)`
  helper does not fetch external scripts; use loader-backed pipeline APIs for
  linked script resources.
- JavaScript module support is side-effect oriented. Webby resolves static
  local/relative imports, runs dependencies before importers, caches modules
  within one page graph, and reports cycles. It does not implement import maps,
  live bindings, namespace imports, dynamic `import()`, top-level `await`, or a
  separate spec-accurate module realm yet.
- Security support is intentionally small: CORS checks cover preloaded
  JavaScript `GET` resources, mixed content is diagnostic-only, CSP is
  diagnostic/no-op, iframe `sandbox` is diagnostic-only, and referrer policy is
  not implemented.
