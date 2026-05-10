# Decision 0001: Build a from-scratch engine

## Status

Accepted.

## Context

Webby is intended to be a serious learning and engineering project. The goal is to understand and implement the browser pipeline rather than wrap an existing engine.

## Decision

Webby will not use Chromium, Blink, CEF, Electron, WebKit, Gecko, Servo, or platform WebView for page rendering.

External libraries are allowed for infrastructure:

- window/event loop
- software pixel buffer or GPU drawing API
- font rasterization/shaping
- HTTP client
- URL parsing
- error handling
- testing helpers

The following components should be implemented in-repo:

- address/search resolution policy
- DOM model
- basic HTML parsing
- visible text extraction
- style defaults and later CSS parsing
- layout tree
- display list
- rendering integration
- browser navigation state

## Consequences

- Webby will not render most modern websites correctly in early versions.
- Google homepage compatibility is not a v0.1 requirement.
- Simple pages like `example.com`, local fixtures, and documentation pages are the initial target.
- The project will be more educational and technically impressive than a wrapper browser.
