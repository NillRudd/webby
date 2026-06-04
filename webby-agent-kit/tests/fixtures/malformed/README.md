# Malformed Input Corpus

These local fixtures exercise forgiving recovery through the public
loader-backed page pipeline:

- `html-recovery.html` keeps readable content after broken nesting and repeated
  less-than characters.
- `html-optional-end-tags.html` exercises optional paragraph, list, and table
  closures plus escapable raw text and foreign-content fallback.
- `css-recovery.html` keeps valid CSS after malformed selectors.
- `js-recovery.html` reports a script diagnostic without losing page output.
- `network-boundaries.html` reports missing linked resources while preserving a
  deterministic page and image placeholder.

The fixtures are intentionally small and original. They are recovery cases,
not alternate engine code paths.
