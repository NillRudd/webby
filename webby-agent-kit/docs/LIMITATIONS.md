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
- Keyboard focus traversal/activation for visible links and form controls, plus
  inspectable accessibility metadata for basic roles.
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
- Open Shadow DOM v0.1 and autonomous custom elements with deterministic
  `connectedCallback` support.
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
- Accelerated rendering backends. Webby has a render-backend abstraction, but
  the deterministic software backend is the only implemented backend.
- No audio/video playback, seeking, buffering, codecs, or media controls beyond
  static placeholders. GIF/WebP decoding and favicon loading are not yet
  implemented.
- Dynamic fetch URL expressions, real Promise scheduling, request bodies,
  credentials modes, preflight requests, and full Fetch/XHR semantics.
- Full DOM event coverage, including broad JavaScript keyboard event support.
- Spec-complete DOM APIs. Current JavaScript DOM bindings cover common
  traversal, selector, class/dataset/style, and layout-geometry reads, but not
  the full browser DOM surface.
- Full Web Components: closed shadow roots, slots, adopted stylesheets,
  customized built-ins, attribute/lifecycle callbacks beyond
  `connectedCallback`, real upgrade timing, and full shadow encapsulation.
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
- Platform accessibility tree integration, printing, a platform download UI,
  or developer network panel.
- Full iframe platform behavior, including nested iframe loading beyond one
  level, sandbox enforcement, iframe form integration, `srcdoc`, permissions
  policy, referrer policy, and cross-frame scripting.

## Simplifications

- Text shaping is deterministic and simple; complex scripts, bidi, ligatures,
  emoji, and platform font matching are not complete.
- Native clipboard and file-dialog integration are not implemented. The shell
  has deterministic app-local copy/paste, explicit local-path opening, and a
  deterministic download manager. Top-level HTML responses render; attachment,
  unsupported-media, and unknown-media responses download. Download progress is
  represented by deterministic completion diagnostics rather than a live
  progress UI. RFC-complete `Content-Disposition` parsing, including
  `filename*`, is not implemented yet.
- HTML and CSS recovery is deliberately forgiving rather than spec-complete.
  Malformed-input fixtures and generated boundary tests guard progress and
  deterministic diagnostics. HTML parsing recovers common omitted paragraph,
  list-item, and table end tags and keeps foreign content as a generic DOM
  fallback, but Webby does not implement the full HTML5 tree-construction,
  namespace, or adoption-agency algorithms.
- CSS cascade supports scoped inheritance, `inherit`, `initial`, `unset`, and
  `!important` for Webby's property subset. It does not implement the complete
  browser origin/layer model, custom properties, or all shorthand expansion.
- Large-document tests prove bounded deterministic behavior, not browser-grade
  performance.
- Cookie `Expires` marks a cookie as persistent, but the date is not interpreted
  yet. `Max-Age=0` or negative values delete matching cookies.
- HTTP Basic authentication supports in-memory credentials for app navigation
  and `--basic-auth user:password` for CLI resource commands. Webby does not
  provide a native credential prompt UI, password manager, persistent credential
  storage, digest authentication, bearer tokens, or credentialed Fetch/XHR
  modes yet.
- The memory-first resource cache and optional profile disk tier key by
  requested URL string and ignore HTTP cache headers. The disk tier uses a
  deterministic JSON index and numbered body files; it is not an HTTP cache.
- Network loading follows up to 10 redirects, detects simple redirect loops,
  times out after 15 seconds by default, and bounds retained response bodies to
  8 MiB. Gzip is supported; brotli and deflate response decoding are not
  enabled yet.
- Optional real-site smoke tests render downloaded single-file HTML snapshots.
  Linked stylesheets, scripts, images, and fonts are not mirrored unless a test
  fixture explicitly provides them, so missing linked-resource diagnostics are
  triage evidence rather than automatic engine regressions.
- CSS Grid support is intentionally small: no `repeat()`, named lines,
  subgrid, dense auto-placement, `span` placement syntax, or advanced
  intrinsic track sizing.
- Overflow clipping and nested wheel scrolling are block-container scoped.
  Scrollbars are not painted yet, horizontal wheel gestures are not routed by
  the native adapter, and inline overflow formatting remains simplified.
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
- Shadow DOM rendering is composed-tree oriented: non-empty shadow children
  replace light DOM children for style/layout/render. Shadow-root styles are
  deterministic but simplified and not fully encapsulated.
- Security support is intentionally small: CORS checks cover preloaded
  JavaScript `GET` resources, mixed content is diagnostic-only, CSP is
  diagnostic/no-op, iframe `sandbox` is diagnostic-only, and referrer policy is
  not implemented.
