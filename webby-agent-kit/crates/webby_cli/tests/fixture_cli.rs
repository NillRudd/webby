use std::path::{Path, PathBuf};
use std::process::Command;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn fixture_path(parts: &[&str]) -> PathBuf {
    let mut path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    for part in parts {
        path.push(part);
    }
    path
}

fn run_cli(args: &[String]) -> TestResult<std::process::Output> {
    let output = Command::new(env!("CARGO_BIN_EXE_webby_cli"))
        .args(args)
        .output()?;
    Ok(output)
}

fn assert_success(output: &std::process::Output) -> TestResult {
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "CLI command failed: status={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    )
    .into())
}

#[test]
fn cli_dump_commands_work_against_fixture_paths() -> TestResult {
    let input = fixture_path(&["sites", "layout-showcase", "index.html"]);
    let input = input.display().to_string();
    let commands: Vec<Vec<String>> = vec![
        vec!["--dump-dom".to_string(), input.clone()],
        vec!["--dump-style".to_string(), input.clone()],
        vec![
            "--dump-layout".to_string(),
            input.clone(),
            "--viewport-width".to_string(),
            "240".to_string(),
        ],
        vec![
            "--dump-display-list".to_string(),
            input.clone(),
            "--viewport-width".to_string(),
            "240".to_string(),
        ],
    ];

    for args in commands {
        let output = run_cli(&args)?;
        assert_success(&output)?;
        assert!(!output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }
    Ok(())
}

#[test]
fn cli_render_ppm_works_against_fixture_path() -> TestResult {
    let input = fixture_path(&["sites", "image-gallery", "index.html"]);
    let output_path =
        std::env::temp_dir().join(format!("webby-fixture-render-{}.ppm", std::process::id()));
    let _ = std::fs::remove_file(&output_path);
    let args = vec![
        "--render-ppm".to_string(),
        input.display().to_string(),
        "--viewport-width".to_string(),
        "240".to_string(),
        "--output".to_string(),
        output_path.display().to_string(),
    ];

    let output = run_cli(&args)?;
    assert_success(&output)?;
    let ppm = std::fs::read(&output_path)?;
    let _ = std::fs::remove_file(&output_path);

    assert!(ppm.starts_with(b"P6\n240 "));
    assert!(ppm.windows(3).any(|pixel| pixel == [255, 0, 0]));
    Ok(())
}

#[test]
fn cli_dumps_are_deterministic_for_fixture_paths() -> TestResult {
    let input = fixture_path(&["sites", "layout-showcase", "index.html"]);
    let args = vec![
        "--dump-layout".to_string(),
        input.display().to_string(),
        "--viewport-width".to_string(),
        "220".to_string(),
    ];

    let first = run_cli(&args)?;
    let second = run_cli(&args)?;
    assert_success(&first)?;
    assert_success(&second)?;
    assert_eq!(first.stdout, second.stdout);
    assert!(first.stderr.is_empty());
    assert!(second.stderr.is_empty());
    Ok(())
}

#[test]
fn cli_diagnostics_dump_reports_fixture_stylesheet_failure() -> TestResult {
    let input = fixture_path(&["regressions", "external-diagnostics.html"]);
    let args = vec![
        "--dump-diagnostics".to_string(),
        input.display().to_string(),
        "--viewport-width".to_string(),
        "180".to_string(),
    ];

    let output = run_cli(&args)?;
    assert_success(&output)?;
    let stdout = String::from_utf8(output.stdout)?;

    assert!(stdout.contains("diagnostics count="));
    assert!(stdout.contains("missing.css could not be loaded"));
    assert!(stdout.contains("bad.css"));
    Ok(())
}

#[test]
fn checked_in_demo_ppm_is_reproducible() -> TestResult {
    let input = fixture_path(&["render", "color-block.html"]);
    let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/demo/color-block.ppm");
    let output_path =
        std::env::temp_dir().join(format!("webby-demo-color-block-{}.ppm", std::process::id()));
    let _ = std::fs::remove_file(&output_path);
    let args = vec![
        "--render-ppm".to_string(),
        input.display().to_string(),
        "--viewport-width".to_string(),
        "96".to_string(),
        "--output".to_string(),
        output_path.display().to_string(),
    ];

    let output = run_cli(&args)?;
    assert_success(&output)?;
    let expected_bytes = std::fs::read(expected)?;
    let actual_bytes = std::fs::read(&output_path)?;
    let _ = std::fs::remove_file(&output_path);

    assert_eq!(expected_bytes, actual_bytes);
    Ok(())
}
