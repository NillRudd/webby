# Module Spec: webby_net

## Purpose

Load resources from URLs.

## Public API sketch

```rust
pub struct ResourceResponse {
    pub requested_url: url::Url,
    pub final_url: url::Url,
    pub status: Option<u16>,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

pub trait ResourceLoader {
    fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse>;
}
```

## Requirements

- Support `http`, `https`, and `file`.
- Do not panic on failed requests.
- Return useful metadata.
- Keep networking out of parser/layout/rendering crates.

## Tests

- `file://` fixture loading.
- invalid scheme handling.
- text decoding helper tests.
