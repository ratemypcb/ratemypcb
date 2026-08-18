use ratemypcb_core::{
    CoverageStatus, NativeMode, Preset, ReviewOptions, Severity, report_schema, review,
};
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

mod viewer;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn help() {
    println!(
        r#"RateMyPCB — local PCB manufacturing preflight

USAGE:
  ratemypcb review [PATH] [--board PATH] [--format terminal|json]
                   [--output FILE] [--preset standard|compact|relaxed]
                   [--fail-on critical|high|medium|low|info|never]
                   [--native auto|off|required] [--open]
  ratemypcb doctor
  ratemypcb schema [--output FILE]
  ratemypcb version

The review never modifies or uploads PCB data. Native DRC is optional.
--open launches a short-lived, offline viewer on 127.0.0.1."#
    );
}

#[derive(Clone, Copy)]
enum OutputFormat {
    Terminal,
    Json,
}

struct ReviewArgs {
    path: PathBuf,
    board: Option<String>,
    format: OutputFormat,
    output: Option<PathBuf>,
    preset: Preset,
    fail_on: Option<Severity>,
    native: NativeMode,
    open: bool,
}

fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_review(args: &[String]) -> Result<ReviewArgs, String> {
    let mut parsed = ReviewArgs {
        path: PathBuf::from("."),
        board: None,
        format: OutputFormat::Terminal,
        output: None,
        preset: Preset::named("standard").unwrap(),
        fail_on: None,
        native: NativeMode::Auto,
        open: false,
    };
    let mut path_set = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--board" => parsed.board = Some(value(args, &mut i, "--board")?),
            "--format" => {
                parsed.format = match value(args, &mut i, "--format")?.as_str() {
                    "terminal" => OutputFormat::Terminal,
                    "json" => OutputFormat::Json,
                    other => return Err(format!("Unsupported format: {other}")),
                }
            }
            "--output" => parsed.output = Some(PathBuf::from(value(args, &mut i, "--output")?)),
            "--preset" => {
                let name = value(args, &mut i, "--preset")?;
                parsed.preset =
                    Preset::named(&name).ok_or_else(|| format!("Unsupported preset: {name}"))?;
            }
            "--fail-on" => {
                let threshold = value(args, &mut i, "--fail-on")?;
                parsed.fail_on = if threshold == "never" {
                    None
                } else {
                    Some(
                        Severity::parse(&threshold)
                            .ok_or_else(|| format!("Unsupported severity: {threshold}"))?,
                    )
                };
            }
            "--native" => {
                parsed.native = match value(args, &mut i, "--native")?.as_str() {
                    "auto" => NativeMode::Auto,
                    "off" => NativeMode::Off,
                    "required" => NativeMode::Required,
                    other => return Err(format!("Unsupported native mode: {other}")),
                }
            }
            "--open" => parsed.open = true,
            "-h" | "--help" => {
                help();
                std::process::exit(0);
            }
            flag if flag.starts_with('-') => return Err(format!("Unknown option: {flag}")),
            path if !path_set => {
                parsed.path = PathBuf::from(path);
                path_set = true;
            }
            other => return Err(format!("Unexpected argument: {other}")),
        }
        i += 1;
    }
    Ok(parsed)
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const LIME: &str = "\x1b[38;5;154m";
const GREEN: &str = "\x1b[38;5;78m";
const ORANGE: &str = "\x1b[38;5;208m";
const RED: &str = "\x1b[38;5;203m";
const BLUE: &str = "\x1b[38;5;75m";

fn paint(enabled: bool, style: &str, text: impl AsRef<str>) -> String {
    if enabled {
        format!("{style}{}{RESET}", text.as_ref())
    } else {
        text.as_ref().to_owned()
    }
}

fn wrapped(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let needed = usize::from(!current.is_empty()) + word.chars().count();
        if !current.is_empty() && current.chars().count() + needed > width {
            lines.push(current);
            current = String::new();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn push_detail(out: &mut String, marker: &str, text: &str) {
    for (index, line) in wrapped(text, 78).into_iter().enumerate() {
        out.push_str(if index == 0 { marker } else { "       " });
        out.push_str(&line);
        out.push('\n');
    }
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "CRITICAL",
        Severity::High => "HIGH",
        Severity::Medium => "MEDIUM",
        Severity::Low => "LOW",
        Severity::Info => "INFO",
    }
}

fn severity_color(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical | Severity::High => RED,
        Severity::Medium => ORANGE,
        Severity::Low => BLUE,
        Severity::Info => DIM,
    }
}

fn render_terminal(report: &ratemypcb_core::Report, color: bool) -> String {
    let score_style = if report.score.value >= 8.5 {
        LIME
    } else if report.score.value >= 6.0 {
        ORANGE
    } else {
        RED
    };
    let passed = report
        .coverage
        .iter()
        .filter(|item| matches!(item.status, CoverageStatus::Passed))
        .count();
    let mut out = String::new();
    out.push_str(&paint(
        color,
        DIM,
        "╭─ RATEMYPCB / LOCAL MANUFACTURING PREFLIGHT ───────────────────────╮\n",
    ));
    out.push_str("│  ");
    out.push_str(&paint(
        color,
        &format!("{BOLD}{score_style}"),
        format!("{:.1} / 10", report.score.value),
    ));
    out.push_str("   ");
    out.push_str(&paint(color, BOLD, report.score.verdict.to_uppercase()));
    out.push('\n');
    out.push_str("│  ");
    out.push_str(&paint(
        color,
        BLUE,
        format!("{} CONFIDENCE", report.confidence.to_uppercase()),
    ));
    out.push_str(&paint(color, DIM, format!("  ·  {}", report.input.kind)));
    out.push('\n');
    out.push_str(&paint(
        color,
        DIM,
        "╰────────────────────────────────────────────────────────────────────╯\n",
    ));
    for (index, line) in wrapped(&report.input.path, 78).into_iter().enumerate() {
        out.push_str(if index == 0 {
            "\nINPUT     "
        } else {
            "          "
        });
        out.push_str(&paint(color, DIM, line));
        out.push('\n');
    }

    out.push_str(&format!(
        "\n{}  {}\n",
        paint(color, BOLD, "COVERAGE"),
        paint(
            color,
            DIM,
            format!("{passed}/{} checks passed", report.coverage.len())
        )
    ));
    for item in &report.coverage {
        let (symbol, status, style) = match item.status {
            CoverageStatus::Passed => ("✓", "PASSED", GREEN),
            CoverageStatus::Attention => ("!", "ATTENTION", ORANGE),
            CoverageStatus::NotRun => ("–", "NOT RUN", DIM),
            CoverageStatus::NotProvided => ("○", "NOT PROVIDED", DIM),
        };
        out.push_str(&format!(
            "  {}  {}  {}\n",
            paint(color, style, symbol),
            paint(color, style, format!("{status:<12}")),
            paint(color, BOLD, &item.label)
        ));
        push_detail(&mut out, "     └ ", &item.evidence);
    }

    out.push_str(&format!(
        "\n{}  {}\n",
        paint(color, BOLD, "FINDINGS"),
        paint(
            color,
            DIM,
            format!(
                "{} issue{}",
                report.findings.len(),
                if report.findings.len() == 1 { "" } else { "s" }
            )
        )
    ));
    if report.findings.is_empty() {
        out.push_str(&format!(
            "  {}  No deterministic findings in the checks that ran.\n",
            paint(color, GREEN, "✓")
        ));
    }
    for (index, finding) in report.findings.iter().enumerate() {
        out.push_str(&format!(
            "  {}  {}  {}\n",
            paint(color, DIM, format!("{:>2}", index + 1)),
            paint(
                color,
                severity_color(finding.severity),
                format!("{:<8}", severity_label(finding.severity))
            ),
            paint(color, BOLD, &finding.title)
        ));
        push_detail(&mut out, "      ", &finding.evidence);
        let recommendation = format!("→ {}", finding.recommendation);
        for line in wrapped(&recommendation, 78) {
            out.push_str("      ");
            out.push_str(&paint(color, LIME, line));
            out.push('\n');
        }
        out.push_str(&format!(
            "      {}\n",
            paint(
                color,
                DIM,
                format!("{} · {} · {}", finding.category, finding.id, finding.source)
            )
        ));
    }

    out.push_str(&format!("\n{}\n", paint(color, BOLD, "LIMITATIONS")));
    for item in &report.limitations {
        for (index, line) in wrapped(item, 80).into_iter().enumerate() {
            out.push_str(if index == 0 { "  · " } else { "    " });
            out.push_str(&paint(color, DIM, line));
            out.push('\n');
        }
    }
    out.push_str(&format!(
        "\n{}  {}\n",
        paint(color, LIME, "VIEW LOCALLY"),
        paint(
            color,
            BOLD,
            "Re-run with --open for the private browser viewer."
        )
    ));
    out.push_str(&format!("\n{}\n", paint(color, DIM, &report.disclaimer)));
    out
}

fn terminal_color_enabled(output: Option<&Path>) -> bool {
    if output.is_some() || std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var("CLICOLOR_FORCE").is_ok_and(|value| value != "0") {
        return true;
    }
    std::io::stdout().is_terminal()
        && !std::env::var("TERM").is_ok_and(|terminal| terminal == "dumb")
}

fn write_output(output: Option<&Path>, content: &str) -> Result<(), String> {
    if let Some(path) = output {
        fs::write(path, content).map_err(|e| format!("Cannot write {}: {e}", path.display()))
    } else {
        print!("{content}");
        Ok(())
    }
}

fn run_review(args: &[String]) -> i32 {
    let parsed = match parse_review(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ratemypcb: {e}");
            return 2;
        }
    };
    let report = match review(
        &parsed.path,
        ReviewOptions {
            board: parsed.board,
            preset: parsed.preset,
            native: parsed.native,
            tool_version: VERSION.into(),
        },
    ) {
        Ok(v) => v,
        Err(ratemypcb_core::Error::Invalid(e) | ratemypcb_core::Error::Ambiguous(e)) => {
            eprintln!("ratemypcb: {e}");
            return 2;
        }
        Err(e) => {
            eprintln!("ratemypcb: {e}");
            return 3;
        }
    };
    let content = match parsed.format {
        OutputFormat::Terminal => {
            render_terminal(&report, terminal_color_enabled(parsed.output.as_deref()))
        }
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(&report).unwrap()),
    };
    if let Err(e) = write_output(parsed.output.as_deref(), &content) {
        eprintln!("ratemypcb: {e}");
        return 3;
    }
    if parsed.open {
        if let Err(error) = viewer::open(&parsed.path, &report) {
            eprintln!("ratemypcb: local viewer: {error}");
            return 3;
        }
    }
    if parsed.fail_on.is_some_and(|threshold| {
        report
            .findings
            .iter()
            .any(|finding| finding.severity >= threshold)
    }) {
        1
    } else {
        0
    }
}

fn doctor() -> i32 {
    let native = std::process::Command::new("kicad-cli")
        .arg("version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    println!(
        "RateMyPCB {VERSION}\nReport schema: {}\nStandalone review: ready\nKiCad source: supported\nFabrication ZIP: supported\nAltium source DRC: unsupported\nkicad-cli: {}",
        ratemypcb_core::SCHEMA_VERSION,
        native.as_deref().unwrap_or("not found (optional)")
    );
    0
}

fn schema(args: &[String]) -> i32 {
    let mut output = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--output" {
            match value(args, &mut i, "--output") {
                Ok(v) => output = Some(PathBuf::from(v)),
                Err(e) => {
                    eprintln!("ratemypcb: {e}");
                    return 2;
                }
            }
        } else {
            eprintln!("ratemypcb: unknown schema option {}", args[i]);
            return 2;
        }
        i += 1;
    }
    let content = format!(
        "{}\n",
        serde_json::to_string_pretty(&report_schema()).unwrap()
    );
    if let Err(e) = write_output(output.as_deref(), &content) {
        eprintln!("ratemypcb: {e}");
        3
    } else {
        0
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("review") => run_review(&args[1..]),
        Some("doctor") => doctor(),
        Some("schema") => schema(&args[1..]),
        Some("version" | "--version" | "-V") => {
            println!(
                "ratemypcb {VERSION} (schema {})",
                ratemypcb_core::SCHEMA_VERSION
            );
            0
        }
        Some("help" | "--help" | "-h") | None => {
            help();
            0
        }
        Some(other) => {
            eprintln!("ratemypcb: unknown command {other}");
            help();
            2
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_report() -> ratemypcb_core::Report {
        review(
            Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/fixtures/narrow-board.kicad_pcb"
            )),
            ReviewOptions {
                board: None,
                preset: Preset::named("standard").unwrap(),
                native: NativeMode::Off,
                tool_version: "test".into(),
            },
        )
        .unwrap()
    }

    #[test]
    fn defaults_are_local_and_optional_native() {
        let args = parse_review(&[]).unwrap();
        assert!(matches!(args.native, NativeMode::Auto));
        assert!(matches!(args.format, OutputFormat::Terminal));
    }
    #[test]
    fn parses_ci_threshold() {
        let args = parse_review(&[
            ".".into(),
            "--fail-on".into(),
            "high".into(),
            "--native".into(),
            "off".into(),
        ])
        .unwrap();
        assert_eq!(args.fail_on, Some(Severity::High));
        assert!(matches!(args.native, NativeMode::Off));
    }

    #[test]
    fn parses_local_viewer_flag() {
        let args = parse_review(&["board.kicad_pcb".into(), "--open".into()]).unwrap();
        assert!(args.open);
    }

    #[test]
    fn plain_terminal_report_is_structured_and_escape_free() {
        let output = render_terminal(&fixture_report(), false);
        assert!(output.contains("RATEMYPCB / LOCAL MANUFACTURING PREFLIGHT"));
        assert!(output.contains("COVERAGE"));
        assert!(output.contains("FINDINGS"));
        assert!(output.contains("VIEW LOCALLY"));
        assert!(output.contains("→ Increase"));
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn color_terminal_report_uses_ansi_styles() {
        let output = render_terminal(&fixture_report(), true);
        assert!(output.contains("\x1b[38;5;"));
        assert!(output.contains(RESET));
    }
}
