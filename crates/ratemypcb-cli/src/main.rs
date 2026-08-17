use ratemypcb_core::{NativeMode, Preset, ReviewOptions, Severity, report_schema, review};
use std::fs;
use std::path::{Path, PathBuf};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn help() {
    println!(
        r#"RateMyPCB — local PCB manufacturing preflight

USAGE:
  ratemypcb review [PATH] [--board PATH] [--format terminal|json]
                   [--output FILE] [--preset standard|compact|relaxed]
                   [--fail-on critical|high|medium|low|info|never]
                   [--native auto|off|required]
  ratemypcb doctor
  ratemypcb schema [--output FILE]
  ratemypcb version

The review never modifies or uploads PCB data. Native DRC is optional."#
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

fn render_terminal(report: &ratemypcb_core::Report) -> String {
    let mut out = format!(
        "RateMyPCB {:.1}/10 — {}\nConfidence: {}\nInput: {}\n\nCoverage\n",
        report.score.value, report.score.verdict, report.confidence, report.input.path
    );
    for item in &report.coverage {
        out.push_str(&format!(
            "  [{:?}] {} — {}\n",
            item.status, item.label, item.evidence
        ));
    }
    out.push_str("\nFindings\n");
    if report.findings.is_empty() {
        out.push_str("  No deterministic findings in the checks that ran.\n");
    }
    for finding in &report.findings {
        out.push_str(&format!(
            "  [{:?}] {} ({})\n    {}\n    Fix: {}\n",
            finding.severity, finding.title, finding.id, finding.evidence, finding.recommendation
        ));
    }
    out.push_str("\nLimitations\n");
    for item in &report.limitations {
        out.push_str(&format!("  - {item}\n"));
    }
    out.push_str(&format!("\n{}\n", report.disclaimer));
    out
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
        OutputFormat::Terminal => render_terminal(&report),
        OutputFormat::Json => format!("{}\n", serde_json::to_string_pretty(&report).unwrap()),
    };
    if let Err(e) = write_output(parsed.output.as_deref(), &content) {
        eprintln!("ratemypcb: {e}");
        return 3;
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
}
