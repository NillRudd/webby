//! Debug CLI for validating Webby's pipeline crates.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use webby_core::{WebbyError, WebbyResult};
use webby_net::ResourceLoader;
use webby_url::SearchEngine;

const MAX_RENDER_VIEWPORT_WIDTH: f32 = 8192.0;

/// Webby debug and validation CLI.
#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    /// Resolve address/search input into a URL.
    #[arg(long)]
    resolve_input: Option<String>,
    /// Fetch a resource URL.
    #[arg(long)]
    fetch: Option<String>,
    /// Download resource bytes to an explicit output path.
    #[arg(long)]
    download: Option<String>,
    /// HTTP Basic credentials for --fetch or --download as user:password.
    #[arg(long)]
    basic_auth: Option<String>,
    /// Dump a parsed DOM tree for an HTML file.
    #[arg(long)]
    dump_dom: Option<PathBuf>,
    /// Dump visible text for an HTML file.
    #[arg(long)]
    dump_text: Option<PathBuf>,
    /// Dump default/computed style information for an HTML file.
    #[arg(long)]
    dump_style: Option<PathBuf>,
    /// Dump parsed CSS rules and stylesheet diagnostics for an HTML file.
    #[arg(long)]
    dump_css: Option<PathBuf>,
    /// Dump layout information for an HTML file.
    #[arg(long)]
    dump_layout: Option<PathBuf>,
    /// Dump non-fatal pipeline diagnostics for an HTML file.
    #[arg(long)]
    dump_diagnostics: Option<PathBuf>,
    /// Dump display-list information for an HTML file.
    #[arg(long)]
    dump_display_list: Option<PathBuf>,
    /// Render an HTML file to a plain PPM image.
    #[arg(long)]
    render_ppm: Option<PathBuf>,
    /// Viewport width used with --dump-layout.
    #[arg(long)]
    viewport_width: Option<String>,
    /// Output path used with --render-ppm.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Render a snapshot for an HTML file.
    #[arg(long)]
    render_snapshot: Option<PathBuf>,
    /// Output path used with --render-snapshot.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Print persistent Webby profile config.
    #[arg(long)]
    show_config: bool,
    /// Print a deterministic profile data summary.
    #[arg(long)]
    profile_summary: bool,
    /// List persistent successful navigation history.
    #[arg(long)]
    list_history: bool,
    /// List persistent bookmarks.
    #[arg(long)]
    list_bookmarks: bool,
    /// Add a persistent bookmark URL.
    #[arg(long)]
    add_bookmark: Option<String>,
    /// Remove a persistent bookmark URL.
    #[arg(long)]
    remove_bookmark: Option<String>,
    /// Clear persistent cookies from the profile.
    #[arg(long)]
    clear_cookies: bool,
    /// Clear persistent successful navigation history and recent pages.
    #[arg(long)]
    clear_history: bool,
    /// Clear persistent bookmarks.
    #[arg(long)]
    clear_bookmarks: bool,
    /// Clear persistent localStorage from the profile.
    #[arg(long)]
    clear_local_storage: bool,
    /// Clear all persisted browsing data controlled by Webby's profile store.
    #[arg(long)]
    clear_browsing_data: bool,
    /// Clear persistent resource bytes from the profile cache.
    #[arg(long)]
    clear_cache: bool,
    /// Profile directory for persistence commands.
    #[arg(long)]
    profile_dir: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();

    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run(args: Args) -> WebbyResult<()> {
    reject_multiple_commands(&args)?;
    reject_irrelevant_options(&args)?;

    if args.show_config {
        let profile = load_profile(&args)?;
        println!("homepage={}", profile.config.homepage);
        println!("cookies_enabled={}", profile.config.cookies_enabled);
        println!("persist_cookies={}", profile.config.persist_cookies);
        println!(
            "clear_cookies_on_exit={}",
            profile.config.clear_cookies_on_exit
        );
        println!("clear_data_on_exit={}", profile.config.clear_data_on_exit);
        println!("javascript_enabled={}", profile.config.javascript_enabled);
        println!("storage_enabled={}", profile.config.storage_enabled);
        println!("disk_cache_enabled={}", profile.config.disk_cache_enabled);
        println!("profile_dir={}", profile_store(&args)?.root().display());
        return Ok(());
    }

    if args.profile_summary {
        let profile = load_profile(&args)?;
        let store = profile_store(&args)?;
        println!("profile_dir={}", store.root().display());
        println!("history_entries={}", profile.history.entries.len());
        println!("recent_pages={}", profile.recent.pages.len());
        println!("bookmarks={}", profile.bookmarks.bookmarks.len());
        println!("cookies={}", profile.cookies.cookies.len());
        println!(
            "local_storage_origins={}",
            profile.local_storage.origins.len()
        );
        println!("disk_cache_enabled={}", profile.config.disk_cache_enabled);
        println!("storage_enabled={}", profile.config.storage_enabled);
        println!("cookies_enabled={}", profile.config.cookies_enabled);
        return Ok(());
    }

    if args.list_history {
        let profile = load_profile(&args)?;
        for entry in profile.history.entries {
            println!("{entry}");
        }
        return Ok(());
    }

    if args.clear_cookies {
        let store = profile_store(&args)?;
        let mut profile = store.load()?;
        store.clear_cookies(&mut profile)?;
        println!("cleared cookies");
        return Ok(());
    }

    if args.clear_history {
        let store = profile_store(&args)?;
        let mut profile = store.load()?;
        store.clear_history(&mut profile)?;
        println!("cleared history");
        return Ok(());
    }

    if args.clear_bookmarks {
        let store = profile_store(&args)?;
        let mut profile = store.load()?;
        store.clear_bookmarks(&mut profile)?;
        println!("cleared bookmarks");
        return Ok(());
    }

    if args.clear_local_storage {
        let store = profile_store(&args)?;
        let mut profile = store.load()?;
        store.clear_local_storage(&mut profile)?;
        println!("cleared localStorage");
        return Ok(());
    }

    if args.clear_browsing_data {
        let store = profile_store(&args)?;
        let mut profile = store.load()?;
        store.clear_browsing_data(&mut profile)?;
        println!("cleared browsing data");
        return Ok(());
    }

    if args.clear_cache {
        let store = profile_store(&args)?;
        webby_cache::DiskResourceCache::open(store.cache_dir())?.clear()?;
        println!("cleared cache");
        return Ok(());
    }

    if args.list_bookmarks {
        let profile = load_profile(&args)?;
        for bookmark in profile.bookmarks.bookmarks {
            println!("{}", bookmark.url);
        }
        return Ok(());
    }

    if let Some(raw_url) = &args.add_bookmark {
        let url = parse_profile_url(raw_url)?;
        let store = profile_store(&args)?;
        let mut profile = store.load()?;
        store.add_bookmark(&mut profile, &url)?;
        println!("added bookmark {}", url);
        return Ok(());
    }

    if let Some(raw_url) = &args.remove_bookmark {
        let url = parse_profile_url(raw_url)?;
        let store = profile_store(&args)?;
        let mut profile = store.load()?;
        let removed = store.remove_bookmark(&mut profile, &url)?;
        if removed {
            println!("removed bookmark {}", url);
        } else {
            println!("bookmark not found {}", url);
        }
        return Ok(());
    }

    if let Some(input) = args.resolve_input {
        let resolved = webby_url::resolve_input(&input, &SearchEngine::default())?;
        println!("{}", resolved.url);
        return Ok(());
    }

    if let Some(raw_url) = &args.fetch {
        let url = url::Url::parse(raw_url).map_err(|error| WebbyError::Url {
            message: error.to_string(),
        })?;
        let loader = webby_net::DefaultResourceLoader::new()?;
        let response = loader.load_with_headers(&url, &basic_auth_headers(&args)?)?;
        println!(
            "requested={} final={} status={:?} content_type={:?} bytes={}",
            response.requested_url,
            response.final_url,
            response.status,
            response.content_type,
            response.byte_len()
        );
        return Ok(());
    }

    if let Some(raw_url) = &args.download {
        let Some(output) = &args.output else {
            return Err(WebbyError::invalid_input(
                "--download requires --output <path>",
            ));
        };
        let url = url::Url::parse(raw_url).map_err(|error| WebbyError::Url {
            message: error.to_string(),
        })?;
        let loader = webby_net::DefaultResourceLoader::new()?;
        let response = loader.load_with_headers(&url, &basic_auth_headers(&args)?)?;
        std::fs::write(output, &response.bytes).map_err(|source| WebbyError::Io {
            path: Some(output.clone()),
            source,
        })?;
        println!(
            "downloaded={} output={} bytes={}",
            response.final_url,
            output.display(),
            response.byte_len()
        );
        return Ok(());
    }

    if let Some(path) = args.dump_dom {
        print!("{}", dump_dom_file(&path)?);
        return Ok(());
    }

    if let Some(path) = args.dump_text {
        println!("{}", dump_text_file(&path)?);
        return Ok(());
    }

    if let Some(path) = args.dump_style {
        print!("{}", dump_style_file(&path)?);
        return Ok(());
    }

    if let Some(path) = args.dump_css {
        print!("{}", dump_css_file(&path)?);
        return Ok(());
    }

    if let Some(path) = args.dump_layout {
        print!(
            "{}",
            dump_layout_file(&path, args.viewport_width.as_deref())?
        );
        return Ok(());
    }

    if let Some(path) = args.dump_display_list {
        print!(
            "{}",
            dump_display_list_file(&path, args.viewport_width.as_deref())?
        );
        return Ok(());
    }

    if let Some(path) = args.dump_diagnostics {
        print!(
            "{}",
            dump_diagnostics_file(&path, args.viewport_width.as_deref())?
        );
        return Ok(());
    }

    if let Some(path) = args.render_ppm {
        let Some(output) = args.output else {
            return Err(WebbyError::invalid_input(
                "--render-ppm requires --output <path>",
            ));
        };

        for diagnostic in render_ppm_file(&path, args.viewport_width.as_deref(), &output)? {
            eprintln!("diagnostic: {diagnostic}");
        }
        return Ok(());
    }

    if let Some(path) = args.render_snapshot {
        if args.out.is_none() {
            return Err(WebbyError::invalid_input(
                "--render-snapshot requires --out <path>",
            ));
        }

        return unsupported_existing_file_command(
            "render snapshot",
            path,
            "use --render-ppm for Milestone 6 software rendering",
        );
    }

    Err(WebbyError::invalid_input(
        "no command provided; run with --help to see available commands",
    ))
}

fn reject_irrelevant_options(args: &Args) -> WebbyResult<()> {
    if args.viewport_width.is_some()
        && args.dump_layout.is_none()
        && args.dump_display_list.is_none()
        && args.render_ppm.is_none()
        && args.dump_diagnostics.is_none()
    {
        return Err(WebbyError::invalid_input(
            "--viewport-width may only be used with --dump-layout, --dump-display-list, --dump-diagnostics, or --render-ppm",
        ));
    }

    if args.output.is_some() && args.render_ppm.is_none() && args.download.is_none() {
        return Err(WebbyError::invalid_input(
            "--output may only be used with --render-ppm or --download",
        ));
    }

    if args.profile_dir.is_some()
        && !args.show_config
        && !args.profile_summary
        && !args.list_history
        && !args.list_bookmarks
        && args.add_bookmark.is_none()
        && args.remove_bookmark.is_none()
        && !args.clear_cookies
        && !args.clear_history
        && !args.clear_bookmarks
        && !args.clear_local_storage
        && !args.clear_browsing_data
        && !args.clear_cache
    {
        return Err(WebbyError::invalid_input(
            "--profile-dir may only be used with profile commands",
        ));
    }

    if args.basic_auth.is_some() && args.fetch.is_none() && args.download.is_none() {
        return Err(WebbyError::invalid_input(
            "--basic-auth may only be used with --fetch or --download",
        ));
    }

    Ok(())
}

fn reject_multiple_commands(args: &Args) -> WebbyResult<()> {
    let commands = [
        ("--resolve-input", args.resolve_input.is_some()),
        ("--fetch", args.fetch.is_some()),
        ("--download", args.download.is_some()),
        ("--dump-dom", args.dump_dom.is_some()),
        ("--dump-text", args.dump_text.is_some()),
        ("--dump-style", args.dump_style.is_some()),
        ("--dump-css", args.dump_css.is_some()),
        ("--dump-layout", args.dump_layout.is_some()),
        ("--dump-display-list", args.dump_display_list.is_some()),
        ("--dump-diagnostics", args.dump_diagnostics.is_some()),
        ("--render-ppm", args.render_ppm.is_some()),
        ("--render-snapshot", args.render_snapshot.is_some()),
        ("--show-config", args.show_config),
        ("--profile-summary", args.profile_summary),
        ("--list-history", args.list_history),
        ("--list-bookmarks", args.list_bookmarks),
        ("--add-bookmark", args.add_bookmark.is_some()),
        ("--remove-bookmark", args.remove_bookmark.is_some()),
        ("--clear-cookies", args.clear_cookies),
        ("--clear-history", args.clear_history),
        ("--clear-bookmarks", args.clear_bookmarks),
        ("--clear-local-storage", args.clear_local_storage),
        ("--clear-browsing-data", args.clear_browsing_data),
        ("--clear-cache", args.clear_cache),
    ];
    let selected = commands
        .iter()
        .filter_map(|(name, selected)| selected.then_some(*name))
        .collect::<Vec<_>>();

    if selected.len() > 1 {
        return Err(WebbyError::invalid_input(format!(
            "expected exactly one command, got: {}",
            selected.join(", ")
        )));
    }

    Ok(())
}

fn profile_store(args: &Args) -> WebbyResult<webby_state::ProfileStore> {
    match &args.profile_dir {
        Some(path) => Ok(webby_state::ProfileStore::new(path)),
        None => webby_state::ProfileStore::default_user(),
    }
}

fn load_profile(args: &Args) -> WebbyResult<webby_state::BrowserProfile> {
    profile_store(args)?.load()
}

fn parse_profile_url(value: &str) -> WebbyResult<url::Url> {
    url::Url::parse(value).map_err(|error| WebbyError::Url {
        message: format!("invalid profile URL {value:?}: {error}"),
    })
}

fn basic_auth_headers(args: &Args) -> WebbyResult<Vec<(String, String)>> {
    let Some(raw) = args.basic_auth.as_deref() else {
        return Ok(Vec::new());
    };
    let Some((username, password)) = raw.split_once(':') else {
        return Err(WebbyError::invalid_input(
            "--basic-auth must be formatted as user:password",
        ));
    };
    if username.is_empty() {
        return Err(WebbyError::invalid_input(
            "--basic-auth username must not be empty",
        ));
    }
    Ok(vec![webby_net::basic_auth_header(
        &webby_net::BasicCredentials::new(username, password),
    )])
}

fn unsupported_existing_file_command(
    command: &str,
    path: PathBuf,
    message: &str,
) -> WebbyResult<()> {
    if !path.exists() {
        return Err(WebbyError::invalid_input(format!(
            "{command} input does not exist: {}",
            path.display()
        )));
    }

    Err(WebbyError::unsupported(format!(
        "{command} for {} is unsupported: {message}",
        path.display()
    )))
}

fn dump_dom_file(path: &std::path::Path) -> WebbyResult<String> {
    let document = parse_html_file(path)?;
    Ok(webby_html::dump_dom(&document))
}

fn dump_text_file(path: &std::path::Path) -> WebbyResult<String> {
    let document = parse_html_file(path)?;
    Ok(webby_html::extract_visible_text(&document))
}

fn dump_style_file(path: &std::path::Path) -> WebbyResult<String> {
    let loader = webby_net::DefaultResourceLoader::new()?;
    let cache = webby_cache::ResourceCache::new();
    let cached_loader = webby_cache::CachedResourceLoader::new(&loader, &cache);
    let (mut document, base_url, html_diagnostics) = load_html_file_with_url(path, &cached_loader)?;
    let javascript_diagnostics =
        execute_document_scripts(&mut document, &base_url, &cached_loader)?;
    let loaded = webby_stylesheet::load_external_stylesheets(&document, &base_url, &cached_loader);
    let stylesheet = webby_style::compose_document_stylesheet(&document, &loaded.stylesheets);
    let styled = webby_style::style_document_with_css(&document, &stylesheet);
    let cache_diagnostics = cached_loader
        .take_diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.format())
        .collect::<Vec<_>>();
    Ok(format_cli_diagnostics(&collect_stylesheet_diagnostics(
        &[
            html_diagnostics,
            cache_diagnostics,
            loaded.diagnostics,
            javascript_diagnostics,
        ]
        .concat(),
        &stylesheet,
    )) + &webby_style::dump_style_tree(&styled))
}

fn dump_css_file(path: &std::path::Path) -> WebbyResult<String> {
    let loader = webby_net::DefaultResourceLoader::new()?;
    let cache = webby_cache::ResourceCache::new();
    let cached_loader = webby_cache::CachedResourceLoader::new(&loader, &cache);
    let (mut document, base_url, html_diagnostics) = load_html_file_with_url(path, &cached_loader)?;
    let javascript_diagnostics =
        execute_document_scripts(&mut document, &base_url, &cached_loader)?;
    let loaded = webby_stylesheet::load_external_stylesheets(&document, &base_url, &cached_loader);
    let stylesheet = webby_style::compose_document_stylesheet(&document, &loaded.stylesheets);
    let cache_diagnostics = cached_loader
        .take_diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.format())
        .collect::<Vec<_>>();
    let diagnostics = collect_stylesheet_diagnostics(
        &[
            html_diagnostics,
            cache_diagnostics,
            loaded.diagnostics,
            javascript_diagnostics,
        ]
        .concat(),
        &stylesheet,
    );
    Ok(format_cli_diagnostics(&diagnostics) + &format_css_rules(&stylesheet))
}

fn dump_layout_file(path: &std::path::Path, viewport_width: Option<&str>) -> WebbyResult<String> {
    let output = layout_file(path, viewport_width)?;
    Ok(format_cli_diagnostics(&output.diagnostics)
        + &webby_layout::dump_layout_tree(&output.layout))
}

fn dump_diagnostics_file(
    path: &std::path::Path,
    viewport_width: Option<&str>,
) -> WebbyResult<String> {
    let output = layout_file(path, viewport_width)?;
    let mut dump = String::new();
    dump.push_str("diagnostics count=");
    dump.push_str(&output.diagnostics.len().to_string());
    dump.push('\n');
    dump.push_str(&format_cli_diagnostics(&output.diagnostics));
    Ok(dump)
}

fn dump_display_list_file(
    path: &std::path::Path,
    viewport_width: Option<&str>,
) -> WebbyResult<String> {
    let output = layout_file(path, viewport_width)?;
    let display_list = webby_render::build_display_list(&output.layout);
    Ok(format_cli_diagnostics(&output.diagnostics)
        + &webby_render::dump_display_list(&display_list))
}

fn render_ppm_file(
    path: &std::path::Path,
    viewport_width: Option<&str>,
    output: &std::path::Path,
) -> WebbyResult<Vec<String>> {
    let width = parse_viewport_width(viewport_width)?;
    if width > MAX_RENDER_VIEWPORT_WIDTH {
        return Err(WebbyError::invalid_input(format!(
            "--render-ppm viewport width must be at most {} pixels",
            MAX_RENDER_VIEWPORT_WIDTH as u32
        )));
    }

    let layout_output = layout_file_with_width(path, width)?;
    let display_list = webby_render::build_display_list(&layout_output.layout);
    let surface_width = width.ceil().max(1.0) as usize;
    let surface_height = layout_output.layout.scroll_height.ceil().max(1.0) as usize;
    let surface = webby_render::render_with_backend(
        &webby_render::SoftwareRenderBackend,
        &display_list,
        surface_width,
        surface_height,
    )?;
    std::fs::write(output, surface.to_ppm()).map_err(|source| WebbyError::Io {
        path: Some(output.to_path_buf()),
        source,
    })?;
    Ok(layout_output.diagnostics)
}

fn layout_file(
    path: &std::path::Path,
    viewport_width: Option<&str>,
) -> WebbyResult<LayoutFileOutput> {
    let width = parse_viewport_width(viewport_width)?;
    layout_file_with_width(path, width)
}

fn layout_file_with_width(path: &std::path::Path, width: f32) -> WebbyResult<LayoutFileOutput> {
    let loader = webby_net::DefaultResourceLoader::new()?;
    let cache = webby_cache::ResourceCache::new();
    let cached_loader = webby_cache::CachedResourceLoader::new(&loader, &cache);
    let (mut document, base_url, html_diagnostics) = load_html_file_with_url(path, &cached_loader)?;
    let javascript_diagnostics =
        execute_document_scripts(&mut document, &base_url, &cached_loader)?;
    let decoded = webby_image::load_images(&document, &base_url, &cached_loader);
    let images = webby_image::to_layout_image_map(&decoded);
    let loaded = webby_stylesheet::load_external_stylesheets(&document, &base_url, &cached_loader);
    let stylesheet = webby_style::compose_document_stylesheet(&document, &loaded.stylesheets);
    let viewport = webby_layout::Viewport::with_width(width)?;
    let styled = webby_style::style_document_with_css_for_viewport(
        &document,
        &stylesheet,
        width,
        viewport.height,
    );
    let layout = webby_layout::layout_tree_with_images(&styled, viewport, &images)?;
    let cache_diagnostics = cached_loader
        .take_diagnostics()
        .into_iter()
        .map(|diagnostic| diagnostic.format())
        .collect::<Vec<_>>();
    Ok(LayoutFileOutput {
        layout,
        diagnostics: collect_stylesheet_diagnostics(
            &[
                html_diagnostics,
                cache_diagnostics,
                loaded.diagnostics,
                javascript_diagnostics,
            ]
            .concat(),
            &stylesheet,
        ),
    })
}

fn execute_document_scripts<L: webby_net::ResourceLoader>(
    document: &mut webby_dom::Document,
    base_url: &url::Url,
    loader: &L,
) -> WebbyResult<Vec<String>> {
    let loaded = webby_script::load_document_scripts(document, base_url, Some(loader));
    let report = webby_js::execute_scripts_with_dom(
        document,
        &loaded.scripts,
        webby_js::ExecutionOptions::default(),
    )?;
    let mut diagnostics = loaded.diagnostics;
    diagnostics.extend(report.diagnostics);
    Ok(diagnostics)
}

struct LayoutFileOutput {
    layout: webby_layout::LayoutTree,
    diagnostics: Vec<String>,
}

fn collect_stylesheet_diagnostics(
    load_diagnostics: &[String],
    stylesheet: &webby_css::Stylesheet,
) -> Vec<String> {
    let mut diagnostics = load_diagnostics.to_vec();
    diagnostics.extend(stylesheet.diagnostics.iter().map(|diagnostic| {
        format!(
            "CSS diagnostic at byte {}: {}",
            diagnostic.offset, diagnostic.message
        )
    }));
    diagnostics
}

fn format_css_rules(stylesheet: &webby_css::Stylesheet) -> String {
    let mut output = String::new();
    output.push_str("css rules=");
    output.push_str(&stylesheet.rules.len().to_string());
    output.push_str(" diagnostics=");
    output.push_str(&stylesheet.diagnostics.len().to_string());
    output.push('\n');
    for (index, rule) in stylesheet.rules.iter().enumerate() {
        output.push_str("  rule ");
        output.push_str(&index.to_string());
        output.push_str(" selector=");
        output.push_str(&format!("{:?}", rule.selector));
        output.push_str(" specificity=");
        output.push_str(&format!("{:?}", rule.selector.specificity()));
        output.push_str(" declarations=");
        output.push_str(&rule.declarations.len().to_string());
        output.push('\n');
    }
    output
}

fn format_cli_diagnostics(diagnostics: &[String]) -> String {
    let mut output = String::new();
    for diagnostic in diagnostics {
        output.push_str("diagnostic: ");
        output.push_str(diagnostic);
        output.push('\n');
    }
    output
}

fn load_html_file_with_url<L: webby_net::ResourceLoader>(
    path: &std::path::Path,
    loader: &L,
) -> WebbyResult<(webby_dom::Document, url::Url, Vec<String>)> {
    let url = html_file_url(path)?;
    let response = loader.load(&url)?;
    let html = webby_net::decode_text(&response.bytes, response.content_type.as_deref());
    let parsed = webby_html::parse_document_with_diagnostics(&html)?;
    Ok((
        parsed.document,
        response.final_url,
        parsed
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.to_string())
            .collect(),
    ))
}

fn html_file_url(path: &std::path::Path) -> WebbyResult<url::Url> {
    let absolute = path.canonicalize().map_err(|source| WebbyError::Io {
        path: Some(path.to_path_buf()),
        source,
    })?;
    let url = url::Url::from_file_path(&absolute).map_err(|()| WebbyError::Url {
        message: format!(
            "could not convert HTML path to file URL: {}",
            absolute.display()
        ),
    })?;
    Ok(url)
}

fn parse_viewport_width(value: Option<&str>) -> WebbyResult<f32> {
    let Some(raw) = value else {
        return Err(WebbyError::invalid_input(
            "--dump-layout requires --viewport-width <number>",
        ));
    };
    let width = raw.parse::<f32>().map_err(|error| {
        WebbyError::invalid_input(format!("invalid --viewport-width value {raw:?}: {error}"))
    })?;

    if !width.is_finite() || width <= 0.0 {
        return Err(WebbyError::invalid_input(
            "--viewport-width must be a finite positive number",
        ));
    }

    Ok(width)
}

fn parse_html_file(path: &std::path::Path) -> WebbyResult<webby_dom::Document> {
    let bytes = std::fs::read(path).map_err(|source| WebbyError::Io {
        path: Some(path.to_path_buf()),
        source,
    })?;
    let html = webby_net::decode_text_utf8(&bytes);
    webby_html::parse_document(&html)
}

#[cfg(test)]
mod tests {
    use super::{
        Args, basic_auth_headers, dump_css_file, dump_diagnostics_file, dump_display_list_file,
        dump_dom_file, dump_layout_file, dump_style_file, dump_text_file, load_html_file_with_url,
        parse_viewport_width, render_ppm_file, run,
    };
    use base64::Engine;
    use image::{ImageBuffer, ImageFormat, Rgba};
    use std::cell::Cell;
    use webby_core::WebbyError;
    use webby_net::{ResourceLoader, ResourceResponse};

    fn empty_args() -> Args {
        Args {
            resolve_input: None,
            fetch: None,
            download: None,
            basic_auth: None,
            dump_dom: None,
            dump_text: None,
            dump_style: None,
            dump_css: None,
            dump_layout: None,
            dump_display_list: None,
            dump_diagnostics: None,
            render_ppm: None,
            viewport_width: None,
            output: None,
            render_snapshot: None,
            out: None,
            show_config: false,
            profile_summary: false,
            list_history: false,
            list_bookmarks: false,
            add_bookmark: None,
            remove_bookmark: None,
            clear_cookies: false,
            clear_history: false,
            clear_bookmarks: false,
            clear_local_storage: false,
            clear_browsing_data: false,
            clear_cache: false,
            profile_dir: None,
        }
    }

    #[test]
    fn cli_reports_missing_command() {
        let result = run(empty_args());

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn render_snapshot_requires_output_path() {
        let mut args = empty_args();
        args.render_snapshot = Some("examples/simple.html".into());

        let result = run(args);

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn render_snapshot_reports_render_ppm_path() {
        let mut args = empty_args();
        args.render_snapshot = Some(fixture_path("layout.html"));
        args.out = Some(fixture_path("snapshot.ppm"));

        let result = run(args);

        assert!(matches!(
            result,
            Err(WebbyError::Unsupported { message })
                if message.contains("--render-ppm") && !message.contains("pending")
        ));
    }

    #[test]
    fn cli_module_docs_use_render_ppm_as_milestone_6_path() {
        let docs = include_str!("../../../docs/MODULES.md");

        assert!(docs.contains("--render-ppm"));
        assert!(!docs.contains("--render-snapshot examples/simple.html --out snapshot.png"));
    }

    #[test]
    fn resolve_input_command_accepts_query() {
        let mut args = empty_args();
        args.resolve_input = Some("rust browser engine".to_string());

        assert!(run(args).is_ok());
    }

    #[test]
    fn resolve_input_command_rejects_empty_input() {
        let mut args = empty_args();
        args.resolve_input = Some("   ".to_string());

        let result = run(args);

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn fetch_command_rejects_invalid_url() {
        let mut args = empty_args();
        args.fetch = Some("not a url".to_string());

        let result = run(args);

        assert!(matches!(result, Err(WebbyError::Url { .. })));
    }

    #[test]
    fn download_command_writes_explicit_output() -> webby_core::WebbyResult<()> {
        let output =
            std::env::temp_dir().join(format!("webby-cli-download-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&output);
        let mut args = empty_args();
        args.download = Some("data:text/plain,hello".to_string());
        args.output = Some(output.clone());

        run(args)?;

        let bytes = std::fs::read(&output).map_err(|source| WebbyError::Io {
            path: Some(output.clone()),
            source,
        })?;
        let _ = std::fs::remove_file(output);
        assert_eq!(bytes, b"hello");
        Ok(())
    }

    #[test]
    fn download_command_requires_output() {
        let mut args = empty_args();
        args.download = Some("data:text/plain,hello".to_string());

        let result = run(args);

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn basic_auth_option_builds_redactable_header_for_resource_commands()
    -> webby_core::WebbyResult<()> {
        let mut args = empty_args();
        args.fetch = Some("data:text/plain,hello".to_string());
        args.basic_auth = Some("webby:secret".to_string());

        let headers = basic_auth_headers(&args)?;

        assert_eq!(headers.len(), 1);
        assert_eq!(headers[0].0, "Authorization");
        assert!(
            format!("{:?}", webby_net::redact_request_headers(&headers)).contains("<redacted>")
        );
        assert!(!format!("{headers:?}").contains("secret"));
        Ok(())
    }

    #[test]
    fn basic_auth_option_is_rejected_for_non_resource_commands() {
        let mut args = empty_args();
        args.dump_text = Some(fixture_path("simple.html"));
        args.basic_auth = Some("webby:secret".to_string());

        let result = run(args);

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn malformed_basic_auth_option_is_structured_error() {
        let mut args = empty_args();
        args.fetch = Some("data:text/plain,hello".to_string());
        args.basic_auth = Some("missing-colon".to_string());

        let result = run(args);

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn multiple_command_flags_are_rejected() {
        let mut args = empty_args();
        args.resolve_input = Some("example.com".to_string());
        args.dump_text = Some(fixture_path("simple.html"));

        let result = run(args);

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn viewport_width_is_rejected_for_non_layout_commands() {
        let mut args = empty_args();
        args.dump_text = Some(fixture_path("simple.html"));
        args.viewport_width = Some("320".to_string());

        let result = run(args);

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn output_is_rejected_for_non_ppm_commands() {
        let mut args = empty_args();
        args.dump_text = Some(fixture_path("simple.html"));
        args.output = Some(fixture_path("out.ppm"));

        let result = run(args);

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn show_config_uses_profile_defaults_when_missing() -> webby_core::WebbyResult<()> {
        let mut args = empty_args();
        args.show_config = true;
        args.profile_dir = Some(temp_profile_dir("show-config"));

        run(args)
    }

    #[test]
    fn add_list_and_remove_bookmark_commands_update_profile() -> webby_core::WebbyResult<()> {
        let profile_dir = temp_profile_dir("bookmark-commands");
        let mut add = empty_args();
        add.add_bookmark = Some("https://example.test/".to_string());
        add.profile_dir = Some(profile_dir.clone());

        run(add)?;
        let store = webby_state::ProfileStore::new(&profile_dir);
        assert_eq!(store.load()?.bookmarks.bookmarks.len(), 1);

        let mut duplicate = empty_args();
        duplicate.add_bookmark = Some("https://example.test/".to_string());
        duplicate.profile_dir = Some(profile_dir.clone());
        run(duplicate)?;
        assert_eq!(store.load()?.bookmarks.bookmarks.len(), 1);

        let mut list = empty_args();
        list.list_bookmarks = true;
        list.profile_dir = Some(profile_dir.clone());
        run(list)?;

        let mut remove = empty_args();
        remove.remove_bookmark = Some("https://example.test/".to_string());
        remove.profile_dir = Some(profile_dir);
        run(remove)?;

        assert!(store.load()?.bookmarks.bookmarks.is_empty());
        Ok(())
    }

    #[test]
    fn clear_cookies_command_updates_profile() -> webby_core::WebbyResult<()> {
        let profile_dir = temp_profile_dir("clear-cookies-command");
        let store = webby_state::ProfileStore::new(&profile_dir);
        let url = url::Url::parse("https://example.test/").map_err(|error| WebbyError::Url {
            message: error.to_string(),
        })?;
        let mut profile = webby_state::BrowserProfile {
            config: webby_state::BrowserConfig {
                persist_cookies: true,
                ..webby_state::BrowserConfig::default()
            },
            ..webby_state::BrowserProfile::default()
        };
        profile.cookies.store_from_headers(
            &url,
            &[(
                "set-cookie".to_string(),
                "persist=yes; Path=/; Max-Age=60".to_string(),
            )],
        );
        store.save(&profile)?;

        let mut args = empty_args();
        args.clear_cookies = true;
        args.profile_dir = Some(profile_dir);
        run(args)?;

        assert!(store.load()?.cookies.cookies.is_empty());
        Ok(())
    }

    #[test]
    fn clear_cache_command_removes_persisted_resource_bytes() -> webby_core::WebbyResult<()> {
        let profile_dir = temp_profile_dir("clear-cache-command");
        let store = webby_state::ProfileStore::new(&profile_dir);
        let disk = webby_cache::DiskResourceCache::open(store.cache_dir())?;
        let memory = webby_cache::ResourceCache::new();
        let loader = CountingSingleResourceLoader::new(
            "https://example.test/page",
            "text/plain",
            b"cached".to_vec(),
        );
        let url =
            url::Url::parse("https://example.test/page").map_err(|error| WebbyError::Url {
                message: error.to_string(),
            })?;
        webby_cache::CachedResourceLoader::new(&loader, &memory)
            .with_disk_cache(&disk)
            .load(&url)?;
        let mut args = empty_args();
        args.clear_cache = true;
        args.profile_dir = Some(profile_dir);

        run(args)?;

        assert!(
            std::fs::read_dir(store.cache_dir())
                .map(|entries| entries.count() == 0)
                .unwrap_or(false)
        );
        Ok(())
    }

    #[test]
    fn privacy_clear_commands_update_profile_data() -> webby_core::WebbyResult<()> {
        let profile_dir = temp_profile_dir("privacy-clear-commands");
        let store = webby_state::ProfileStore::new(&profile_dir);
        let url = url::Url::parse("https://example.test/").map_err(|error| WebbyError::Url {
            message: error.to_string(),
        })?;
        let mut profile = webby_state::BrowserProfile::default();
        store.record_successful_navigation(&mut profile, &url)?;
        store.add_bookmark(&mut profile, &url)?;
        profile
            .local_storage
            .set_item("https://example.test:443", "theme", "dark")?;
        store.save(&profile)?;

        let mut summary = empty_args();
        summary.profile_summary = true;
        summary.profile_dir = Some(profile_dir.clone());
        run(summary)?;

        let mut clear_history = empty_args();
        clear_history.clear_history = true;
        clear_history.profile_dir = Some(profile_dir.clone());
        run(clear_history)?;
        assert!(store.load()?.history.entries.is_empty());
        assert!(store.load()?.recent.pages.is_empty());

        let mut clear_bookmarks = empty_args();
        clear_bookmarks.clear_bookmarks = true;
        clear_bookmarks.profile_dir = Some(profile_dir.clone());
        run(clear_bookmarks)?;
        assert!(store.load()?.bookmarks.bookmarks.is_empty());

        let mut clear_local_storage = empty_args();
        clear_local_storage.clear_local_storage = true;
        clear_local_storage.profile_dir = Some(profile_dir);
        run(clear_local_storage)?;
        assert!(store.load()?.local_storage.origins.is_empty());
        Ok(())
    }

    #[test]
    fn clear_browsing_data_command_clears_profile_without_corrupting_config()
    -> webby_core::WebbyResult<()> {
        let profile_dir = temp_profile_dir("clear-browsing-data-command");
        let store = webby_state::ProfileStore::new(&profile_dir);
        let url = url::Url::parse("https://example.test/").map_err(|error| WebbyError::Url {
            message: error.to_string(),
        })?;
        let mut profile = webby_state::BrowserProfile {
            config: webby_state::BrowserConfig {
                homepage: "https://home.example/".to_string(),
                ..webby_state::BrowserConfig::default()
            },
            ..webby_state::BrowserProfile::default()
        };
        store.record_successful_navigation(&mut profile, &url)?;
        store.add_bookmark(&mut profile, &url)?;
        profile
            .local_storage
            .set_item("https://example.test:443", "theme", "dark")?;
        store.save(&profile)?;

        let mut args = empty_args();
        args.clear_browsing_data = true;
        args.profile_dir = Some(profile_dir);
        run(args)?;
        let loaded = store.load()?;

        assert_eq!(loaded.config.homepage, "https://home.example/");
        assert!(loaded.history.entries.is_empty());
        assert!(loaded.bookmarks.bookmarks.is_empty());
        assert!(loaded.local_storage.origins.is_empty());
        Ok(())
    }

    #[test]
    fn list_history_command_reads_persistent_history() -> webby_core::WebbyResult<()> {
        let profile_dir = temp_profile_dir("history-command");
        let store = webby_state::ProfileStore::new(&profile_dir);
        let mut profile = webby_state::BrowserProfile::default();
        let url = url::Url::parse("https://example.test/").map_err(|error| WebbyError::Url {
            message: error.to_string(),
        })?;
        store.record_successful_navigation(&mut profile, &url)?;
        let mut args = empty_args();
        args.list_history = true;
        args.profile_dir = Some(profile_dir);

        run(args)
    }

    #[test]
    fn profile_dir_is_rejected_for_non_profile_commands() {
        let mut args = empty_args();
        args.dump_text = Some(fixture_path("simple.html"));
        args.profile_dir = Some(temp_profile_dir("irrelevant-profile"));

        let result = run(args);

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn dump_text_command_reads_fixture() {
        let mut args = empty_args();
        args.dump_text = Some(fixture_path("simple.html"));

        assert!(run(args).is_ok());
    }

    #[test]
    fn dump_dom_command_reports_missing_file() {
        let mut args = empty_args();
        args.dump_dom = Some(fixture_path("missing.html"));

        let result = run(args);

        assert!(matches!(result, Err(WebbyError::Io { .. })));
    }

    #[test]
    fn dump_dom_file_contains_stable_tree_lines() -> webby_core::WebbyResult<()> {
        let dump = dump_dom_file(&fixture_path("simple.html"))?;

        assert!(dump.contains("#document\n"));
        assert!(dump.contains("  <html>\n"));
        assert!(dump.contains("      <title>\n"));
        assert!(dump.contains("        <a href=\"https://example.com\">\n"));
        Ok(())
    }

    #[test]
    fn dump_text_ignores_head_script_and_style_for_fixture() -> webby_core::WebbyResult<()> {
        let dump = dump_text_file(&fixture_path("simple.html"))?;

        assert!(dump.contains("Hello from Webby"));
        assert!(!dump.contains("Webby Simple Page"));
        assert!(!dump.contains("font-size"));
        Ok(())
    }

    #[test]
    fn dump_style_command_reads_fixture() {
        let mut args = empty_args();
        args.dump_style = Some(fixture_path("simple.html"));

        assert!(run(args).is_ok());
    }

    #[test]
    fn dump_style_file_contains_stable_style_lines() -> webby_core::WebbyResult<()> {
        let dump = dump_style_file(&fixture_path("simple.html"))?;

        assert!(dump.contains("<body> display=block"));
        assert!(dump.contains("<h1> display=block"));
        assert!(dump.contains("<a href=\"https://example.com\"> display=inline"));
        assert!(dump.contains("text-decoration=underline"));
        assert!(!dump.contains("<style>"));
        Ok(())
    }

    #[test]
    fn dump_style_reflects_css_rules() -> webby_core::WebbyResult<()> {
        let path = write_temp_html(
            "webby-css-style",
            "<style>p.note { color: red; padding: 3px; }</style><p class=\"note\">Styled</p>",
        )?;

        let dump = dump_style_file(&path)?;
        let _ = std::fs::remove_file(&path);

        assert!(dump.contains("<p class=\"note\"> display=block color=rgba(255,0,0,255)"));
        assert!(dump.contains("padding=3.0/3.0/3.0/3.0"));
        assert!(!dump.contains("p.note"));
        Ok(())
    }

    #[test]
    fn dump_style_reflects_milestone_13_selector_behavior() -> webby_core::WebbyResult<()> {
        let path = write_temp_html(
            "webby-css-selectors",
            "<style>.card > p.highlighted[name=\"q\"] { color: red; }</style><div class=\"card\"><p class=\"highlighted\" name=\"q\">Selected</p></div>",
        )?;

        let dump = dump_style_file(&path)?;
        let _ = std::fs::remove_file(&path);

        assert!(dump.contains(
            "<p class=\"highlighted\" name=\"q\"> display=block color=rgba(255,0,0,255)"
        ));
        Ok(())
    }

    #[test]
    fn dump_style_applies_external_stylesheets() -> webby_core::WebbyResult<()> {
        let css = write_temp_css("webby-external-style", "p { color: red; }")?;
        let css_name = temp_filename(&css)?;
        let html = write_temp_html(
            "webby-external-style",
            &format!("<link rel=\"stylesheet\" href=\"{css_name}\"><p>External</p>"),
        )?;

        let dump = dump_style_file(&html)?;
        let _ = std::fs::remove_file(&css);
        let _ = std::fs::remove_file(&html);

        assert!(dump.contains("<p> display=block color=rgba(255,0,0,255)"));
        Ok(())
    }

    #[test]
    fn dump_css_command_is_deterministic() -> webby_core::WebbyResult<()> {
        let html = write_temp_html(
            "webby-debug-css",
            "<style>p.card { color: red; margin: 2px; }</style><body><p class=\"card\">Hi</p></body>",
        )?;

        let first = dump_css_file(&html)?;
        let second = dump_css_file(&html)?;

        assert_eq!(first, second);
        assert!(first.contains("css rules="));
        assert!(first.contains("selector="));
        assert!(first.contains("declarations=2"));
        Ok(())
    }

    #[test]
    fn dump_diagnostics_command_is_deterministic_and_includes_stylesheet_failures()
    -> webby_core::WebbyResult<()> {
        let html = write_temp_html(
            "webby-debug-diagnostics",
            "<link rel=\"stylesheet\" href=\"missing.css\"><body><img src=\"missing.png\"><form><input name=\"q\"></form></body>",
        )?;

        let first = dump_diagnostics_file(&html, Some("180"))?;
        let second = dump_diagnostics_file(&html, Some("180"))?;

        assert_eq!(first, second);
        assert!(first.contains("diagnostics count="));
        assert!(first.contains("diagnostic: stylesheet file://"));
        assert!(first.contains("missing.css could not be loaded"));
        Ok(())
    }

    #[test]
    fn dump_diagnostics_includes_javascript_console_and_errors() -> webby_core::WebbyResult<()> {
        let html = write_temp_html(
            "webby-js-diagnostics",
            "<body><script>console.log('cli', 31);</script><script>document.querySelector('div + p')</script><p>Visible</p></body>",
        )?;

        let dump = dump_diagnostics_file(&html, Some("180"))?;
        let _ = std::fs::remove_file(&html);

        assert!(dump.contains("diagnostic: JavaScript error in inline script 2"));
        assert!(dump.contains("diagnostic: JavaScript console: cli 31"));
        assert!(dump.contains("diagnostics count="));
        Ok(())
    }

    #[test]
    fn dump_diagnostics_includes_html_parser_recovery() -> webby_core::WebbyResult<()> {
        let html = write_temp_html("webby-html-diagnostics", "<body>Visible<!-- missing")?;
        let dump = dump_diagnostics_file(&html, Some("180"))?;
        let _ = std::fs::remove_file(&html);

        assert!(dump.contains(
            "diagnostic: HTML diagnostic at byte 13: unclosed comment ignored through end of input"
        ));
        Ok(())
    }

    #[test]
    fn dump_style_surfaces_failed_stylesheet_load_without_failing() -> webby_core::WebbyResult<()> {
        let html = write_temp_html(
            "webby-missing-style",
            "<link rel=\"stylesheet\" href=\"missing.css\"><p>Visible</p>",
        )?;

        let dump = dump_style_file(&html)?;
        let _ = std::fs::remove_file(&html);

        assert!(dump.contains("diagnostic: stylesheet file://"));
        assert!(dump.contains("missing.css could not be loaded:"));
        assert!(dump.contains("<p> display=block"));
        Ok(())
    }

    #[test]
    fn dump_style_surfaces_malformed_external_css_without_failing() -> webby_core::WebbyResult<()> {
        let css = write_temp_css("webby-malformed-style", "p { color: red; } broken")?;
        let css_name = temp_filename(&css)?;
        let html = write_temp_html(
            "webby-malformed-style",
            &format!("<link rel=\"stylesheet\" href=\"{css_name}\"><p>Visible</p>"),
        )?;

        let dump = dump_style_file(&html)?;
        let _ = std::fs::remove_file(&css);
        let _ = std::fs::remove_file(&html);

        assert!(dump.contains("diagnostic: stylesheet file://"));
        assert!(dump.contains(&css_name));
        assert!(dump.contains("CSS diagnostic at byte "));
        assert!(dump.contains("skipped CSS without declaration block"));
        assert!(dump.contains("<p> display=block color=rgba(255,0,0,255)"));
        Ok(())
    }

    #[test]
    fn dump_style_executes_inline_scripts_without_affecting_visible_tree()
    -> webby_core::WebbyResult<()> {
        let html = write_temp_html(
            "webby-js-style",
            "<body><script>console.log('style dump');</script><p>Visible</p></body>",
        )?;

        let dump = dump_style_file(&html)?;
        let _ = std::fs::remove_file(&html);

        assert!(dump.contains("diagnostic: JavaScript console: style dump"));
        assert!(dump.contains("<p> display=block"));
        Ok(())
    }

    #[test]
    fn dump_style_reflects_javascript_dom_mutation() -> webby_core::WebbyResult<()> {
        let html = write_temp_html(
            "webby-js-style-mutation",
            "<style>.hot { color: red; }</style><body><p id=\"intro\">Text</p><script>document.getElementById('intro').className = 'hot';</script></body>",
        )?;

        let dump = dump_style_file(&html)?;
        let _ = std::fs::remove_file(&html);

        assert!(
            dump.contains("<p class=\"hot\" id=\"intro\"> display=block color=rgba(255,0,0,255)")
        );
        Ok(())
    }

    #[test]
    fn dump_style_executes_external_scripts_in_loader_path() -> webby_core::WebbyResult<()> {
        let script = write_temp_js(
            "webby-external-script",
            "document.getElementById('intro').className = 'hot';",
        )?;
        let script_name = temp_filename(&script)?;
        let html = write_temp_html(
            "webby-external-script",
            &format!(
                "<style>.hot {{ color: red; }}</style><p id=\"intro\">Text</p><script src=\"{script_name}\"></script>"
            ),
        )?;

        let dump = dump_style_file(&html)?;
        let _ = std::fs::remove_file(&script);
        let _ = std::fs::remove_file(&html);

        assert!(
            dump.contains("<p class=\"hot\" id=\"intro\"> display=block color=rgba(255,0,0,255)")
        );
        Ok(())
    }

    #[test]
    fn dump_style_executes_module_scripts_in_loader_path() -> webby_core::WebbyResult<()> {
        let module = write_temp_js(
            "webby-module-script",
            "document.getElementById('intro').className = 'hot';",
        )?;
        let module_name = temp_filename(&module)?;
        let html = write_temp_html(
            "webby-module-script",
            &format!(
                "<style>.hot {{ color: red; }}</style><p id=\"intro\">Text</p><script type=\"module\" src=\"{module_name}\"></script>"
            ),
        )?;

        let dump = dump_style_file(&html)?;
        let _ = std::fs::remove_file(&module);
        let _ = std::fs::remove_file(&html);

        assert!(
            dump.contains("<p class=\"hot\" id=\"intro\"> display=block color=rgba(255,0,0,255)")
        );
        Ok(())
    }

    #[test]
    fn dump_style_reports_failed_external_script_load() -> webby_core::WebbyResult<()> {
        let html = write_temp_html(
            "webby-missing-script",
            "<script src=\"missing.js\"></script><p>Visible</p>",
        )?;

        let dump = dump_style_file(&html)?;
        let _ = std::fs::remove_file(&html);

        assert!(dump.contains("diagnostic: JavaScript skipped external script 1 file://"));
        assert!(dump.contains("missing.js: failed to load script:"));
        assert!(dump.contains("<p> display=block"));
        Ok(())
    }

    #[test]
    fn dump_layout_requires_viewport_width() {
        let mut args = empty_args();
        args.dump_layout = Some(fixture_path("layout.html"));

        let result = run(args);

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn invalid_viewport_width_returns_structured_cli_error() {
        let result = parse_viewport_width(Some("wide"));

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn dump_layout_command_reads_fixture() {
        let mut args = empty_args();
        args.dump_layout = Some(fixture_path("layout.html"));
        args.viewport_width = Some("320".to_string());

        assert!(run(args).is_ok());
    }

    #[test]
    fn dump_layout_file_contains_stable_lines() -> webby_core::WebbyResult<()> {
        let dump = dump_layout_file(&fixture_path("layout.html"), Some("320"))?;

        assert!(dump.contains("layout scroll-height="));
        assert!(dump.contains("body kind=block"));
        assert!(dump.contains("h1 kind=block"));
        assert!(dump.contains("text \"Layout\""));
        assert!(dump.contains("text \"Test\""));
        Ok(())
    }

    #[test]
    fn dump_layout_reflects_inline_line_structure() -> webby_core::WebbyResult<()> {
        let html = write_temp_html(
            "webby-inline-layout",
            "<body><p>before <span>span</span><img width=\"10\" height=\"8\"> after</p></body>",
        )?;

        let dump = dump_layout_file(&html, Some("180"))?;
        let _ = std::fs::remove_file(&html);

        assert!(dump.contains("line rect="));
        assert!(dump.contains("baseline="));
        assert!(dump.contains("text \"before\""));
        assert!(dump.contains("img kind=image-placeholder"));
        assert!(dump.contains("text \"after\""));
        Ok(())
    }

    #[test]
    fn dump_layout_uses_viewport_width_for_media_queries() -> webby_core::WebbyResult<()> {
        let html = write_temp_html(
            "webby-media-layout",
            "<style>
                p { font-size: 12px; }
                @media screen and (max-width: 120px) { p { font-size: 30px; } }
            </style><body><p>Responsive</p></body>",
        )?;

        let narrow = dump_layout_file(&html, Some("100"))?;
        let wide = dump_layout_file(&html, Some("240"))?;
        let _ = std::fs::remove_file(&html);

        assert_ne!(narrow, wide);
        assert!(narrow.contains("font-size=30.0"));
        assert!(wide.contains("font-size=12.0"));
        Ok(())
    }

    #[test]
    fn dump_layout_includes_form_controls() -> webby_core::WebbyResult<()> {
        let html = write_temp_html(
            "webby-form-layout",
            "<body><form action=\"/find\"><input type=\"search\" name=\"q\"><input type=\"submit\" value=\"Go\"></form></body>",
        )?;

        let dump = dump_layout_file(&html, Some("220"))?;
        let _ = std::fs::remove_file(&html);

        assert!(dump.contains("input kind=form-control"));
        assert!(dump.contains("control-type=search"));
        assert!(dump.contains("form-controls"));
        assert!(dump.contains("type=submit"));
        Ok(())
    }

    #[test]
    fn dump_layout_surfaces_failed_stylesheet_load_without_failing() -> webby_core::WebbyResult<()>
    {
        let html = write_temp_html(
            "webby-missing-layout-style",
            "<link rel=\"stylesheet\" href=\"missing.css\"><p>Visible</p>",
        )?;

        let dump = dump_layout_file(&html, Some("120"))?;
        let _ = std::fs::remove_file(&html);

        assert!(dump.contains("diagnostic: stylesheet file://"));
        assert!(dump.contains("missing.css could not be loaded:"));
        assert!(dump.contains("layout scroll-height="));
        Ok(())
    }

    #[test]
    fn dump_display_list_command_reads_fixture() {
        let mut args = empty_args();
        args.dump_display_list = Some(fixture_path("layout.html"));
        args.viewport_width = Some("320".to_string());

        assert!(run(args).is_ok());
    }

    #[test]
    fn dump_display_list_file_contains_stable_lines() -> webby_core::WebbyResult<()> {
        let dump = dump_display_list_file(&fixture_path("layout.html"), Some("320"))?;

        assert!(dump.contains("display-list commands="));
        assert!(dump.contains("text \"Layout\""));
        assert!(dump.contains("font-weight=bold"));
        Ok(())
    }

    #[test]
    fn dump_display_list_surfaces_failed_stylesheet_load_without_failing()
    -> webby_core::WebbyResult<()> {
        let html = write_temp_html(
            "webby-missing-display-style",
            "<link rel=\"stylesheet\" href=\"missing.css\"><p>Visible</p>",
        )?;

        let dump = dump_display_list_file(&html, Some("120"))?;
        let _ = std::fs::remove_file(&html);

        assert!(dump.contains("diagnostic: stylesheet file://"));
        assert!(dump.contains("missing.css could not be loaded:"));
        assert!(dump.contains("display-list commands="));
        Ok(())
    }

    #[test]
    fn render_ppm_requires_output_path() {
        let mut args = empty_args();
        args.render_ppm = Some(fixture_path("layout.html"));
        args.viewport_width = Some("320".to_string());

        let result = run(args);

        assert!(matches!(result, Err(WebbyError::InvalidInput { .. })));
    }

    #[test]
    fn huge_render_ppm_viewport_width_is_rejected_before_render_allocation() {
        let mut args = empty_args();
        args.render_ppm = Some(fixture_path("layout.html"));
        args.viewport_width = Some("999999".to_string());
        args.output = Some(fixture_path("huge.ppm"));

        let result = run(args);

        assert!(matches!(
            result,
            Err(WebbyError::InvalidInput { message })
                if message.contains("--render-ppm viewport width")
        ));
    }

    #[test]
    fn render_ppm_writes_output_file() -> webby_core::WebbyResult<()> {
        let output =
            std::env::temp_dir().join(format!("webby-render-ppm-{}.ppm", std::process::id()));
        let _ = std::fs::remove_file(&output);
        let mut args = empty_args();
        args.render_ppm = Some(fixture_path("layout.html"));
        args.viewport_width = Some("64".to_string());
        args.output = Some(output.clone());

        run(args)?;
        let bytes = std::fs::read(&output).map_err(|source| WebbyError::Io {
            path: Some(output.clone()),
            source,
        })?;
        let _ = std::fs::remove_file(&output);

        assert!(bytes.starts_with(b"P6\n64 "));
        Ok(())
    }

    #[test]
    fn render_ppm_returns_stylesheet_diagnostics_for_stderr() -> webby_core::WebbyResult<()> {
        let html = write_temp_html(
            "webby-missing-render-style",
            "<link rel=\"stylesheet\" href=\"missing.css\"><body>Visible</body>",
        )?;
        let output = std::env::temp_dir().join(format!(
            "webby-missing-render-style-{}.ppm",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&output);

        let diagnostics = render_ppm_file(&html, Some("64"), &output)?;
        let bytes = std::fs::read(&output).map_err(|source| WebbyError::Io {
            path: Some(output.clone()),
            source,
        })?;
        let _ = std::fs::remove_file(&html);
        let _ = std::fs::remove_file(&output);

        assert!(bytes.starts_with(b"P6\n64 "));
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("stylesheet file://")
                    && diagnostic.contains("missing.css could not be loaded:"))
        );
        Ok(())
    }

    #[test]
    fn cli_html_loading_path_can_use_shared_resource_cache() -> webby_core::WebbyResult<()> {
        let html = write_temp_html("webby-cli-cache", "<body>cached</body>")?;
        let canonical = html.canonicalize().map_err(|source| WebbyError::Io {
            path: Some(html.clone()),
            source,
        })?;
        let file_url = url::Url::from_file_path(&canonical).map_err(|()| WebbyError::Url {
            message: "temporary HTML path could not become a file URL".to_string(),
        })?;
        let loader = CountingSingleResourceLoader::new(
            file_url.as_str(),
            "text/html; charset=utf-8",
            b"<body>cached</body>".to_vec(),
        );
        let cache = webby_cache::ResourceCache::new();
        let cached_loader = webby_cache::CachedResourceLoader::new(&loader, &cache);

        load_html_file_with_url(&html, &cached_loader)?;
        load_html_file_with_url(&html, &cached_loader)?;
        let diagnostics = cached_loader.take_diagnostics();
        let _ = std::fs::remove_file(&html);

        assert_eq!(loader.calls.get(), 1);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.format() == format!("cache hit {file_url}"))
        );
        Ok(())
    }

    #[test]
    fn render_ppm_uses_css_applied_visual_changes() -> webby_core::WebbyResult<()> {
        let input = write_temp_html(
            "webby-css-render",
            "<style>body { background-color: red; }</style><body>CSS</body>",
        )?;
        let output =
            std::env::temp_dir().join(format!("webby-css-render-{}.ppm", std::process::id()));
        let _ = std::fs::remove_file(&output);
        let mut args = empty_args();
        args.render_ppm = Some(input.clone());
        args.viewport_width = Some("64".to_string());
        args.output = Some(output.clone());

        run(args)?;
        let bytes = std::fs::read(&output).map_err(|source| WebbyError::Io {
            path: Some(output.clone()),
            source,
        })?;
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);

        assert!(bytes.windows(3).any(|pixel| pixel == [255, 0, 0]));
        Ok(())
    }

    #[test]
    fn render_ppm_includes_form_control_pixels() -> webby_core::WebbyResult<()> {
        let input = write_temp_html(
            "webby-form-render",
            "<body><form><input type=\"text\" name=\"q\" value=\"webby\"><button type=\"submit\">Go</button></form></body>",
        )?;
        let output =
            std::env::temp_dir().join(format!("webby-form-render-{}.ppm", std::process::id()));
        let _ = std::fs::remove_file(&output);
        let mut args = empty_args();
        args.render_ppm = Some(input.clone());
        args.viewport_width = Some("160".to_string());
        args.output = Some(output.clone());

        run(args)?;
        let bytes = std::fs::read(&output).map_err(|source| WebbyError::Io {
            path: Some(output.clone()),
            source,
        })?;
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);

        assert!(bytes.starts_with(b"P6\n160 "));
        assert!(bytes.windows(3).any(|pixel| pixel == [128, 128, 128]));
        Ok(())
    }

    #[test]
    fn render_ppm_applies_external_stylesheets() -> webby_core::WebbyResult<()> {
        let css = write_temp_css("webby-external-render", "body { background-color: red; }")?;
        let css_name = temp_filename(&css)?;
        let input = write_temp_html(
            "webby-external-render",
            &format!("<link rel=\"stylesheet\" href=\"{css_name}\"><body>External</body>"),
        )?;
        let output =
            std::env::temp_dir().join(format!("webby-external-render-{}.ppm", std::process::id()));
        let _ = std::fs::remove_file(&output);
        let mut args = empty_args();
        args.render_ppm = Some(input.clone());
        args.viewport_width = Some("64".to_string());
        args.output = Some(output.clone());

        run(args)?;
        let bytes = std::fs::read(&output).map_err(|source| WebbyError::Io {
            path: Some(output.clone()),
            source,
        })?;
        let _ = std::fs::remove_file(&css);
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);

        assert!(bytes.windows(3).any(|pixel| pixel == [255, 0, 0]));
        Ok(())
    }

    #[test]
    fn render_ppm_includes_image_pixels() -> webby_core::WebbyResult<()> {
        let image_path =
            std::env::temp_dir().join(format!("webby-cli-image-{}.png", std::process::id()));
        std::fs::write(&image_path, encoded_test_image(ImageFormat::Png)?).map_err(|source| {
            WebbyError::Io {
                path: Some(image_path.clone()),
                source,
            }
        })?;
        let html = format!(
            "<body><img src=\"{}\"></body>",
            image_path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| WebbyError::invalid_input("temp image filename was not UTF-8"))?
        );
        let input = write_temp_html("webby-image-render", &html)?;
        let output =
            std::env::temp_dir().join(format!("webby-image-render-{}.ppm", std::process::id()));
        let _ = std::fs::remove_file(&output);
        let mut args = empty_args();
        args.render_ppm = Some(input.clone());
        args.viewport_width = Some("64".to_string());
        args.output = Some(output.clone());

        run(args)?;
        let bytes = std::fs::read(&output).map_err(|source| WebbyError::Io {
            path: Some(output.clone()),
            source,
        })?;
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&image_path);
        let _ = std::fs::remove_file(&output);

        assert!(bytes.windows(3).any(|pixel| pixel == [255, 0, 0]));
        Ok(())
    }

    #[test]
    fn render_ppm_includes_data_url_image_pixels() -> webby_core::WebbyResult<()> {
        let image =
            base64::engine::general_purpose::STANDARD.encode(encoded_test_image(ImageFormat::Png)?);
        let input = write_temp_html(
            "webby-data-image-render",
            &format!("<body><img src=\"data:image/png;base64,{image}\"></body>"),
        )?;
        let output = std::env::temp_dir().join(format!(
            "webby-data-image-render-{}.ppm",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&output);
        let mut args = empty_args();
        args.render_ppm = Some(input.clone());
        args.viewport_width = Some("64".to_string());
        args.output = Some(output.clone());

        run(args)?;
        let bytes = std::fs::read(&output).map_err(|source| WebbyError::Io {
            path: Some(output.clone()),
            source,
        })?;
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);

        assert!(bytes.windows(3).any(|pixel| pixel == [255, 0, 0]));
        Ok(())
    }

    fn fixture_path(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name)
    }

    fn temp_profile_dir(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("webby-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    fn write_temp_html(name: &str, html: &str) -> webby_core::WebbyResult<std::path::PathBuf> {
        let path = std::env::temp_dir().join(format!("{name}-{}.html", std::process::id()));
        std::fs::write(&path, html).map_err(|source| WebbyError::Io {
            path: Some(path.clone()),
            source,
        })?;
        Ok(path)
    }

    fn write_temp_css(name: &str, css: &str) -> webby_core::WebbyResult<std::path::PathBuf> {
        let path = std::env::temp_dir().join(format!("{name}-{}.css", std::process::id()));
        std::fs::write(&path, css).map_err(|source| WebbyError::Io {
            path: Some(path.clone()),
            source,
        })?;
        Ok(path)
    }

    fn write_temp_js(name: &str, js: &str) -> webby_core::WebbyResult<std::path::PathBuf> {
        let path = std::env::temp_dir().join(format!("{name}-{}.js", std::process::id()));
        std::fs::write(&path, js).map_err(|source| WebbyError::Io {
            path: Some(path.clone()),
            source,
        })?;
        Ok(path)
    }

    fn temp_filename(path: &std::path::Path) -> webby_core::WebbyResult<String> {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .ok_or_else(|| WebbyError::invalid_input("temporary filename was not UTF-8"))
    }

    fn encoded_test_image(format: ImageFormat) -> webby_core::WebbyResult<Vec<u8>> {
        let image = ImageBuffer::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 255, 0, 255])
            }
        });
        let mut cursor = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut cursor, format)
            .map_err(|error| WebbyError::Render {
                message: format!("failed to encode test image: {error}"),
            })?;
        Ok(cursor.into_inner())
    }

    #[derive(Debug)]
    struct CountingSingleResourceLoader {
        url: String,
        content_type: String,
        bytes: Vec<u8>,
        calls: Cell<usize>,
    }

    impl CountingSingleResourceLoader {
        fn new(url: &str, content_type: &str, bytes: Vec<u8>) -> Self {
            Self {
                url: url.to_string(),
                content_type: content_type.to_string(),
                bytes,
                calls: Cell::new(0),
            }
        }
    }

    impl ResourceLoader for CountingSingleResourceLoader {
        fn load(&self, url: &url::Url) -> webby_core::WebbyResult<ResourceResponse> {
            self.calls.set(self.calls.get().saturating_add(1));
            if self.url == url.as_str() {
                return Ok(ResourceResponse {
                    requested_url: url.clone(),
                    final_url: url.clone(),
                    status: Some(200),
                    content_type: Some(self.content_type.clone()),
                    headers: Vec::new(),
                    bytes: self.bytes.clone(),
                });
            }

            Err(WebbyError::Network {
                message: format!("no counted CLI test resource for {url}"),
            })
        }
    }
}
