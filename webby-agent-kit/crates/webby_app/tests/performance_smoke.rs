use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use webby_app::{LARGE_DOCUMENT_NODE_DIAGNOSTIC_THRESHOLD, PagePipeline};
use webby_core::{WebbyError, WebbyResult};
use webby_js::{ExecutionOptions, ScriptSource};
use webby_layout::Viewport;
use webby_net::DefaultResourceLoader;

const STAGE_BUDGET: Duration = Duration::from_secs(10);
const PIPELINE_BUDGET: Duration = Duration::from_secs(20);
const MAX_IMAGE_HEAVY_SURFACE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug)]
struct StageMeasurements {
    html_parse: Duration,
    css_parse: Duration,
    style_tree: Duration,
    layout: Duration,
    display_list: Duration,
    render: Duration,
    javascript: Duration,
}

#[test]
fn measured_engine_stages_stay_within_documented_smoke_budgets() -> WebbyResult<()> {
    let (html, css) = generated_large_page(280, 180);
    let html_start = Instant::now();
    let document = webby_html::parse_document(&html)?;
    let html_parse = html_start.elapsed();

    let css_start = Instant::now();
    let stylesheet = webby_css::parse_stylesheet(&css)?;
    let css_parse = css_start.elapsed();

    let style_start = Instant::now();
    let styled = webby_style::style_document_with_css(&document, &stylesheet);
    let style_tree = style_start.elapsed();

    let layout_start = Instant::now();
    let layout = webby_layout::layout_tree(&styled, Viewport::new(480.0, 320.0)?)?;
    let layout_duration = layout_start.elapsed();

    let display_start = Instant::now();
    let display_list = webby_render::build_display_list(&layout);
    let display_list_duration = display_start.elapsed();

    let render_start = Instant::now();
    let surface = webby_render::render_to_surface(
        &display_list,
        480,
        layout.scroll_height.ceil().max(1.0) as usize,
    )?;
    let render = render_start.elapsed();

    let javascript_start = Instant::now();
    let javascript_report = webby_js::execute_scripts(
        &[ScriptSource::new(
            "performance smoke",
            "var total = 0; for (var index = 0; index < 100; index += 1) { total += index; }",
        )],
        ExecutionOptions::default(),
    )?;
    let javascript = javascript_start.elapsed();

    let measurements = StageMeasurements {
        html_parse,
        css_parse,
        style_tree,
        layout: layout_duration,
        display_list: display_list_duration,
        render,
        javascript,
    };
    assert_stage_budgets(&measurements);
    assert!(!display_list.commands.is_empty());
    assert!(!surface.pixels.is_empty());
    assert!(javascript_report.diagnostics.is_empty());
    Ok(())
}

#[test]
fn large_document_pipeline_emits_advisory_diagnostic_and_stays_bounded() -> WebbyResult<()> {
    let html = generated_many_node_page(LARGE_DOCUMENT_NODE_DIAGNOSTIC_THRESHOLD);
    let start = Instant::now();
    let page = PagePipeline::new(360, 240).render_html(&html, test_url()?)?;

    assert!(start.elapsed() <= PIPELINE_BUDGET);
    assert!(page.diagnostics.iter().any(|diagnostic| {
        diagnostic.contains("performance diagnostic: document has")
            && diagnostic.contains("advisory threshold")
    }));
    assert!(!page.display_list.commands.is_empty());
    Ok(())
}

fn generated_many_node_page(node_count: usize) -> String {
    let mut html = String::from("<!doctype html><html><body><h1>Large document</h1>");
    for _ in 0..node_count {
        html.push_str("<span></span>");
    }
    html.push_str("</body></html>");
    html
}

#[test]
fn image_heavy_fixture_pipeline_stays_bounded() -> WebbyResult<()> {
    let loader = DefaultResourceLoader::new()?;
    let base = fixture_path(&["corpus", "ecommerce-grid", "index.html"]);
    let base_url = file_url(&base)?;
    let mut html = String::from("<!doctype html><html><body>");
    for _ in 0..80 {
        html.push_str(
            "<img src=\"../../sites/image-gallery/red-green.png\" width=\"36\" height=\"18\" alt=\"sample\">",
        );
    }
    html.push_str("</body></html>");
    let start = Instant::now();
    let page = PagePipeline::new(480, 320).render_html_with_loader(&html, base_url, &loader)?;

    assert!(start.elapsed() <= PIPELINE_BUDGET);
    assert!(page.surface.pixels.len() <= MAX_IMAGE_HEAVY_SURFACE_BYTES);
    assert!(
        page.display_list
            .commands
            .iter()
            .filter(|command| matches!(command, webby_render::DisplayCommand::DrawImage { .. }))
            .count()
            >= 80
    );
    Ok(())
}

fn generated_large_page(article_count: usize, rule_count: usize) -> (String, String) {
    let mut css = String::new();
    for index in 0..rule_count {
        css.push_str(&format!(
            ".rule-{index} {{ margin: 1px; padding: 1px; color: #123456; }}\n"
        ));
    }
    let mut html = String::from("<!doctype html><html><body>");
    for index in 0..article_count {
        html.push_str(&format!(
            "<article class=\"rule-{}\"><h2>Entry {index}</h2><p>Measured pipeline paragraph with deterministic text.</p></article>",
            index % rule_count.max(1)
        ));
    }
    html.push_str("</body></html>");
    (html, css)
}

fn assert_stage_budgets(measurements: &StageMeasurements) {
    for (name, elapsed) in [
        ("HTML parsing", measurements.html_parse),
        ("CSS parsing", measurements.css_parse),
        ("style tree", measurements.style_tree),
        ("layout", measurements.layout),
        ("display list", measurements.display_list),
        ("render", measurements.render),
        ("JavaScript", measurements.javascript),
    ] {
        assert!(
            elapsed <= STAGE_BUDGET,
            "{name} exceeded {STAGE_BUDGET:?}: {elapsed:?}"
        );
    }
}

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

fn test_url() -> WebbyResult<url::Url> {
    url::Url::parse("https://example.test/performance").map_err(|error| WebbyError::Url {
        message: error.to_string(),
    })
}
