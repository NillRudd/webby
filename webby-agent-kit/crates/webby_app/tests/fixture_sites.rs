use std::path::{Path, PathBuf};

use base64::Engine;
use image::{ImageBuffer, ImageFormat, Rgba};
use webby_app::{AppState, CHROME_HEIGHT, PagePipeline};
use webby_core::{WebbyError, WebbyResult};
use webby_layout::FormControlType;
use webby_net::{DefaultResourceLoader, ResourceLoader, ResourceResponse};
use webby_render::{DisplayCommand, build_display_list};

fn fixture_path(parts: &[&str]) -> PathBuf {
    let mut path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    for part in parts {
        path.push(part);
    }
    path
}

fn file_url(path: &Path) -> WebbyResult<url::Url> {
    url::Url::from_file_path(path).map_err(|()| WebbyError::Url {
        message: format!("fixture path could not become file URL: {}", path.display()),
    })
}

fn load_fixture_page(
    parts: &[&str],
    width: usize,
    height: usize,
) -> WebbyResult<webby_app::RenderedPage> {
    let loader = DefaultResourceLoader::new()?;
    let url = file_url(&fixture_path(parts))?;
    PagePipeline::new(width, height).load_url(&loader, &url)
}

fn has_rgb_pixel(page: &webby_app::RenderedPage, rgb: [u8; 3]) -> bool {
    page.surface
        .pixels
        .chunks_exact(4)
        .any(|pixel| pixel[0] == rgb[0] && pixel[1] == rgb[1] && pixel[2] == rgb[2])
}

fn display_text(page: &webby_app::RenderedPage) -> String {
    page.display_list
        .commands
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

#[test]
fn full_pipeline_loads_static_blog_from_file_url() -> WebbyResult<()> {
    let page = load_fixture_page(&["sites", "static-blog", "index.html"], 420, 260)?;

    assert!(page.url.as_str().ends_with("/static-blog/index.html"));
    assert!(!page.display_list.commands.is_empty());
    assert!(page.content_height > 0.0);
    assert!(page.surface.width > 0);
    Ok(())
}

#[test]
fn static_blog_visible_text_contains_expected_content() -> WebbyResult<()> {
    let path = fixture_path(&["sites", "static-blog", "index.html"]);
    let html = std::fs::read_to_string(&path).map_err(|source| WebbyError::Io {
        path: Some(path),
        source,
    })?;
    let document = webby_html::parse_document(&html)?;
    let text = webby_html::extract_visible_text(&document);

    assert!(text.contains("Webby Field Notes"));
    assert!(text.contains("Building a Tiny Browser"));
    assert!(text.contains("Home"));
    assert!(text.contains("About"));
    assert!(text.contains("Fixture blog footer"));
    let expected =
        std::fs::read_to_string(fixture_path(&["expected", "static-blog-visible-text.txt"]))
            .map_err(|source| WebbyError::Io {
                path: Some(fixture_path(&["expected", "static-blog-visible-text.txt"])),
                source,
            })?;
    for line in expected.lines().filter(|line| !line.trim().is_empty()) {
        assert!(text.contains(line));
    }
    Ok(())
}

#[test]
fn static_blog_external_css_affects_display_list_and_render_output() -> WebbyResult<()> {
    let page = load_fixture_page(&["sites", "static-blog", "index.html"], 420, 260)?;

    assert!(page.display_list.commands.iter().any(|command| {
        matches!(
            command,
            DisplayCommand::FillRect { color, .. }
                if color.r == 247 && color.g == 251 && color.b == 255
        )
    }));
    assert!(has_rgb_pixel(&page, [247, 251, 255]));
    Ok(())
}

#[test]
fn static_blog_links_resolve_correctly() -> WebbyResult<()> {
    let page = load_fixture_page(&["sites", "static-blog", "index.html"], 420, 260)?;
    let href = page
        .links
        .iter()
        .find(|link| link.href == "about.html")
        .map(|link| link.href.as_str())
        .ok_or_else(|| {
            WebbyError::invalid_input("static blog fixture did not expose about link")
        })?;
    let resolved = page.url.join(href).map_err(|error| WebbyError::Url {
        message: error.to_string(),
    })?;

    assert!(resolved.as_str().ends_with("/static-blog/about.html"));
    Ok(())
}

#[test]
fn image_gallery_renders_decoded_image_pixels() -> WebbyResult<()> {
    let page = load_fixture_page(&["sites", "image-gallery", "index.html"], 260, 220)?;

    assert!(has_rgb_pixel(&page, [255, 0, 0]));
    assert!(has_rgb_pixel(&page, [0, 220, 0]));
    Ok(())
}

#[test]
fn image_gallery_missing_image_falls_back_deterministically() -> WebbyResult<()> {
    let page = load_fixture_page(&["sites", "image-gallery", "index.html"], 260, 220)?;

    assert!(
        page.display_list
            .commands
            .iter()
            .any(|command| { matches!(command, DisplayCommand::ImagePlaceholder { .. }) })
    );
    assert_eq!(page.display_list, build_display_list(&page.layout));
    Ok(())
}

#[test]
fn form_search_serializes_get_query_from_fixture() -> WebbyResult<()> {
    let loader = DefaultResourceLoader::new()?;
    let start = file_url(&fixture_path(&["sites", "form-search", "index.html"]))?;
    let mut state = AppState::with_window_size(360, 220);

    state.navigate_to_url(&loader, start);
    let control = state
        .page
        .as_ref()
        .and_then(|page| {
            page.form_controls
                .iter()
                .find(|control| control.control_type == FormControlType::Search)
                .cloned()
        })
        .ok_or_else(|| {
            WebbyError::invalid_input("form search fixture did not expose search input")
        })?;
    state.focused_form_control = Some(control.id);
    state
        .form_values
        .insert(control.id, "rust browser".to_string());

    assert!(state.submit_focused_form(&loader));
    let current = state
        .navigation
        .current_url
        .as_ref()
        .ok_or_else(|| WebbyError::invalid_input("form submit did not commit a current URL"))?;
    assert!(
        current
            .as_str()
            .ends_with("/form-search/results.html?q=rust+browser")
    );
    Ok(())
}

#[test]
fn layout_showcase_layout_dump_is_deterministic() -> WebbyResult<()> {
    let page = load_fixture_page(&["sites", "layout-showcase", "index.html"], 220, 260)?;
    let first = webby_layout::dump_layout_tree(&page.layout);
    let second = webby_layout::dump_layout_tree(&page.layout);

    assert_eq!(first, second);
    let expected_lines = std::fs::read_to_string(fixture_path(&[
        "expected",
        "layout-showcase-required-dump-lines.txt",
    ]))
    .map_err(|source| WebbyError::Io {
        path: Some(fixture_path(&[
            "expected",
            "layout-showcase-required-dump-lines.txt",
        ])),
        source,
    })?;
    for line in expected_lines
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        assert!(first.contains(line));
    }
    Ok(())
}

#[test]
fn structured_fixture_groups_exercise_pipeline_stages() -> WebbyResult<()> {
    let loader = DefaultResourceLoader::new()?;
    let pipeline = PagePipeline::new(240, 160);
    for parts in [
        &["html", "basic-document.html"][..],
        &["css", "selector-cascade.html"][..],
        &["layout", "box-model.html"][..],
        &["render", "color-block.html"][..],
        &["forms", "search.html"][..],
        &["images", "sizing.html"][..],
        &["navigation", "page-a.html"][..],
    ] {
        let url = file_url(&fixture_path(parts))?;
        let page = pipeline.load_url(&loader, &url)?;
        assert!(!page.display_list.commands.is_empty());
    }
    Ok(())
}

#[test]
fn dynamic_mini_sites_load_and_match_expected_text() -> WebbyResult<()> {
    let todo = load_fixture_page(&["sites", "js-todo", "index.html"], 320, 180)?;
    let dynamic = load_fixture_page(&["sites", "dynamic-fetch-storage", "index.html"], 320, 180)?;

    assert!(display_text(&todo).contains("Todo Fixture"));
    assert!(display_text(&todo).contains("Initial task"));
    assert!(display_text(&dynamic).contains("Fetched fixture data / session ok"));
    assert!(dynamic.diagnostics.is_empty());
    Ok(())
}

#[test]
fn js_todo_fixture_click_mutates_dom_through_app_pipeline() -> WebbyResult<()> {
    let loader = DefaultResourceLoader::new()?;
    let url = file_url(&fixture_path(&["sites", "js-todo", "index.html"]))?;
    let mut state = AppState::with_window_size(360, 220);

    state.navigate_to_url(&loader, url);
    let button = state
        .page
        .as_ref()
        .and_then(|page| page.form_controls.last().cloned())
        .ok_or_else(|| WebbyError::invalid_input("todo fixture did not expose add button"))?;
    assert!(state.click_at(
        button.rect.x + 1.0,
        button.rect.y + CHROME_HEIGHT as f32 + 1.0,
        &loader
    ));
    let page = state
        .page
        .as_ref()
        .ok_or_else(|| WebbyError::invalid_input("todo click did not keep rendered page"))?;

    assert!(display_text(page).contains("Write fixture tests done"));
    Ok(())
}

#[test]
fn dynamic_fetch_storage_fixture_updates_app_storage() -> WebbyResult<()> {
    let loader = DefaultResourceLoader::new()?;
    let url = file_url(&fixture_path(&[
        "sites",
        "dynamic-fetch-storage",
        "index.html",
    ]))?;
    let mut state = AppState::with_window_size(360, 220);

    state.navigate_to_url(&loader, url.clone());
    let origin = webby_state::storage_origin_key(&url)?;

    assert_eq!(
        state.local_storage.get_item(&origin, "fixture-message"),
        Some("Fetched fixture data\n")
    );
    assert_eq!(
        state.session_storage.get_item(&origin, "fixture-session"),
        Some("session ok")
    );
    let page = state
        .page
        .as_ref()
        .ok_or_else(|| WebbyError::invalid_input("dynamic fixture did not render page"))?;
    let text = display_text(page);
    assert!(text.contains("Fetched fixture data"));
    assert!(text.contains("session ok"));
    Ok(())
}

#[test]
fn fixture_back_forward_and_reload_interactions_are_stable() -> WebbyResult<()> {
    let loader = DefaultResourceLoader::new()?;
    let index = file_url(&fixture_path(&["sites", "static-blog", "index.html"]))?;
    let about = file_url(&fixture_path(&["sites", "static-blog", "about.html"]))?;
    let mut state = AppState::with_window_size(360, 220);

    state.navigate_to_url(&loader, index.clone());
    state.navigate_to_url(&loader, about.clone());
    assert!(state.go_back(&loader));
    assert_eq!(state.navigation.current_url.as_ref(), Some(&index));
    assert!(state.go_forward(&loader));
    assert_eq!(state.navigation.current_url.as_ref(), Some(&about));
    assert!(state.reload(&loader));
    assert_eq!(state.navigation.current_url.as_ref(), Some(&about));
    Ok(())
}

#[test]
fn regression_fixtures_do_not_panic_or_fail_pipeline() -> WebbyResult<()> {
    let loader = DefaultResourceLoader::new()?;
    let pipeline = PagePipeline::new(260, 180);
    for name in [
        "raw-text-lookalikes.html",
        "malformed-selectors.html",
        "external-diagnostics.html",
        "image-decode-fallback.html",
        "form-get.html",
    ] {
        let url = file_url(&fixture_path(&["regressions", name]))?;
        let page = pipeline.load_url(&loader, &url)?;
        assert!(!page.display_list.commands.is_empty());
    }
    Ok(())
}

#[test]
fn malformed_fixture_corpus_recovers_deterministically() -> WebbyResult<()> {
    let loader = DefaultResourceLoader::new()?;
    let pipeline = PagePipeline::new(280, 180);
    for (name, visible_text, expects_diagnostic) in [
        ("html-recovery.html", "HTML recovery stays visible", false),
        ("css-recovery.html", "CSS recovery stays visible", true),
        (
            "js-recovery.html",
            "JavaScript recovery stays visible",
            true,
        ),
        (
            "network-boundaries.html",
            "Linked resource recovery stays visible",
            true,
        ),
    ] {
        let url = file_url(&fixture_path(&["malformed", name]))?;
        let first = pipeline.load_url(&loader, &url)?;
        let second = pipeline.load_url(&loader, &url)?;

        assert!(display_text(&first).contains(visible_text));
        assert!(!first.display_list.commands.is_empty());
        assert_eq!(first.diagnostics, second.diagnostics);
        assert_eq!(first.display_list, second.display_list);
        assert_eq!(
            webby_layout::dump_layout_tree(&first.layout),
            webby_layout::dump_layout_tree(&second.layout)
        );
        assert_eq!(expects_diagnostic, !first.diagnostics.is_empty());
    }
    Ok(())
}

#[test]
fn fixture_navigation_tabs_and_cache_remain_app_level_behaviors() -> WebbyResult<()> {
    let loader = DefaultResourceLoader::new()?;
    let index = file_url(&fixture_path(&["sites", "static-blog", "index.html"]))?;
    let about = file_url(&fixture_path(&["sites", "static-blog", "about.html"]))?;
    let mut state = AppState::with_window_size(360, 220);

    state.navigate_to_url(&loader, index.clone());
    assert_eq!(state.navigation.current_url.as_ref(), Some(&index));
    state.new_tab();
    state.navigate_to_url(&loader, about.clone());
    assert_eq!(state.navigation.current_url.as_ref(), Some(&about));
    assert!(state.switch_tab(0));
    assert_eq!(state.navigation.current_url.as_ref(), Some(&index));
    state.validate_active_tab_sync()?;
    Ok(())
}

#[test]
fn url_input_through_rendered_output_uses_full_app_pipeline() -> WebbyResult<()> {
    let loader = DefaultResourceLoader::new()?;
    let index = fixture_path(&["sites", "static-blog", "index.html"]);
    let mut state = AppState::with_window_size(420, 260);

    state.chrome.set_address_input(index.display().to_string());
    state.submit_address(&loader);

    let page = state
        .page
        .as_ref()
        .ok_or_else(|| WebbyError::invalid_input("address submit did not render a page"))?;
    assert!(matches!(state.status, webby_app::PageStatus::Loaded { .. }));
    assert!(page.url.as_str().ends_with("/static-blog/index.html"));
    assert!(has_rgb_pixel(page, [247, 251, 255]));
    assert!(!page.surface.to_ppm().is_empty());
    Ok(())
}

#[test]
fn large_document_pipeline_stays_bounded_and_deterministic() -> WebbyResult<()> {
    let image = base64::engine::general_purpose::STANDARD.encode(encoded_test_image()?);
    let mut css = String::from("article { margin: 2px; padding: 1px; } .item { color: #123456; }");
    for index in 0..150 {
        css.push_str(".rule-");
        css.push_str(&index.to_string());
        css.push_str(" { margin: 1px; padding: 1px; }\n");
    }
    let mut html = format!("<!doctype html><html><head><style>{css}</style></head><body><section>");
    for _ in 0..30 {
        html.push_str("<img width=\"2\" height=\"1\" src=\"data:image/png;base64,");
        html.push_str(&image);
        html.push_str("\">");
    }
    html.push_str("</section>");
    for index in 0..400 {
        html.push_str("<article class=\"item\"><h2>Entry ");
        html.push_str(&index.to_string());
        html.push_str("</h2><p>Generated paragraph with <a href=\"about.html\">link text</a> and inline content.</p></article>");
    }
    html.push_str("</body></html>");

    let url = file_url(&fixture_path(&["sites", "static-blog", "index.html"]))?;
    let pipeline = PagePipeline::new(360, 480);
    let first = pipeline.render_html_with_loader(&html, url.clone(), &NoopLoader)?;
    let second = pipeline.render_html_with_loader(&html, url, &NoopLoader)?;

    assert!(first.content_height > 1000.0);
    assert!(
        first
            .display_list
            .commands
            .iter()
            .filter(|command| matches!(command, DisplayCommand::DrawImage { .. }))
            .count()
            >= 30
    );
    assert_eq!(
        webby_layout::dump_layout_tree(&first.layout),
        webby_layout::dump_layout_tree(&second.layout)
    );
    assert_eq!(first.display_list, second.display_list);
    Ok(())
}

#[test]
fn reduced_real_world_corpus_matches_compatibility_report() -> WebbyResult<()> {
    let first = corpus_compatibility_report()?;
    let second = corpus_compatibility_report()?;
    let expected_path = fixture_path(&["expected", "corpus-compatibility-report.txt"]);
    let expected = std::fs::read_to_string(&expected_path).map_err(|source| WebbyError::Io {
        path: Some(expected_path),
        source,
    })?;

    assert_eq!(first, second);
    assert_eq!(first, expected);
    Ok(())
}

#[test]
fn responsive_blog_corpus_fixture_applies_narrow_media_rule() -> WebbyResult<()> {
    let page = load_fixture_page(&["corpus", "responsive-blog", "index.html"], 320, 220)?;

    assert!(has_rgb_pixel(&page, [220, 252, 231]));
    Ok(())
}

struct CorpusCase {
    name: &'static str,
    path: &'static str,
    width: usize,
    required_text: &'static [&'static str],
    min_links: usize,
    min_controls: usize,
    unsupported: &'static str,
}

fn corpus_compatibility_report() -> WebbyResult<String> {
    let mut report = String::from("# Webby reduced website corpus compatibility\n");
    for case in corpus_cases() {
        let page = load_fixture_page(&["corpus", case.path, "index.html"], case.width, 260)?;
        let text = display_text(&page);
        let render_passed = case
            .required_text
            .iter()
            .filter(|required| text.contains(**required))
            .count();
        let interaction_checks =
            usize::from(case.min_links > 0) + usize::from(case.min_controls > 0);
        let interaction_passed =
            usize::from(page.links.len() >= case.min_links && case.min_links > 0)
                + usize::from(
                    page.form_controls.len() >= case.min_controls && case.min_controls > 0,
                );
        report.push_str(&format!(
            "{} render={}/{} interaction={}/{} diagnostics={} unsupported={} ppm_fnv1a64={:016x}\n",
            case.name,
            render_passed,
            case.required_text.len(),
            interaction_passed,
            interaction_checks,
            page.diagnostics.len(),
            case.unsupported,
            fnv1a64(&page.surface.to_ppm())
        ));
    }
    Ok(report)
}

fn corpus_cases() -> [CorpusCase; 7] {
    [
        CorpusCase {
            name: "documentation",
            path: "documentation",
            width: 440,
            required_text: &["Acorn", "Install", "Examples"],
            min_links: 2,
            min_controls: 0,
            unsupported: "none",
        },
        CorpusCase {
            name: "news-article",
            path: "news-article",
            width: 440,
            required_text: &["North Harbor Daily", "Community garden opens", "Related"],
            min_links: 2,
            min_controls: 0,
            unsupported: "none",
        },
        CorpusCase {
            name: "responsive-blog",
            path: "responsive-blog",
            width: 320,
            required_text: &["Small Screen Notes", "Responsive by construction"],
            min_links: 1,
            min_controls: 0,
            unsupported: "none",
        },
        CorpusCase {
            name: "search-page",
            path: "search-page",
            width: 360,
            required_text: &["Atlas Search", "Search pages"],
            min_links: 0,
            min_controls: 2,
            unsupported: "none",
        },
        CorpusCase {
            name: "dashboard",
            path: "dashboard",
            width: 440,
            required_text: &["Signal Board", "Requests", "Healthy"],
            min_links: 1,
            min_controls: 0,
            unsupported: "none",
        },
        CorpusCase {
            name: "ecommerce-grid",
            path: "ecommerce-grid",
            width: 440,
            required_text: &["Juniper Supply", "Field notebook", "Trail bottle"],
            min_links: 1,
            min_controls: 3,
            unsupported: "missing-image-placeholder",
        },
        CorpusCase {
            name: "login-form",
            path: "login-form",
            width: 360,
            required_text: &["Lantern Account", "Email", "Password"],
            min_links: 0,
            min_controls: 3,
            unsupported: "authentication-server",
        },
    ]
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn encoded_test_image() -> WebbyResult<Vec<u8>> {
    let image = ImageBuffer::from_fn(2, 1, |x, _| {
        if x == 0 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 255, 0, 255])
        }
    });
    let mut cursor = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|error| WebbyError::Render {
            message: format!("failed to encode fixture smoke image: {error}"),
        })?;
    Ok(cursor.into_inner())
}

struct NoopLoader;

impl ResourceLoader for NoopLoader {
    fn load(&self, url: &url::Url) -> WebbyResult<ResourceResponse> {
        Err(WebbyError::Network {
            message: format!("unexpected fixture load for {url}"),
        })
    }
}
