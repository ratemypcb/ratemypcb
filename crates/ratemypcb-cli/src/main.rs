use ratemypcb_core::{
    ASSESSMENT_SCHEMA_VERSION, Assessment, CoverageStatus, DfmDeclarations, GateImpact, NativeMode,
    Preset, ReviewOptions, ReviewScope, Severity, report_schema, review, validate_assessment,
    validate_report, validate_report_supply_retention,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

mod viewer;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_REPORT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ASSESSMENT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_DFM_DECLARATION_BYTES: u64 = 256 * 1024;

fn read_bounded(path: &Path, label: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if bytes.len() as u64 > max_bytes {
        let (limit, unit) = if max_bytes >= 1024 * 1024 {
            (max_bytes / 1024 / 1024, "MiB")
        } else {
            (max_bytes / 1024, "KiB")
        };
        return Err(format!("{label} exceeds the {limit} {unit} limit"));
    }
    Ok(bytes)
}

fn dfm_declaration_source_path(path: &Path) -> Result<(PathBuf, String), String> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve {}: {error}", path.display()))?;
    let cwd = std::env::current_dir()
        .and_then(fs::canonicalize)
        .map_err(|error| format!("cannot resolve the current directory: {error}"))?;
    let logical = canonical.strip_prefix(&cwd).unwrap_or(&canonical);
    let components = logical
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => value.to_str().map(str::to_owned),
            _ => None,
        })
        .collect::<Vec<_>>();
    if canonical.starts_with(&cwd) && !components.is_empty() {
        return Ok((canonical, components.join("/")));
    }
    let filename = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("DFM declarations require a UTF-8 file name")?
        .to_owned();
    let canonical_text = canonical
        .to_str()
        .ok_or("DFM declarations require a UTF-8 source path")?;
    let digest = format!("{:x}", Sha256::digest(canonical_text.as_bytes()));
    Ok((canonical, format!("external/{}/{filename}", &digest[..16])))
}

fn help() {
    println!(
        r#"RateMyPCB — local PCB manufacturing preflight

USAGE:
  ratemypcb review [PATH] [--board PATH] [--schematic PATH]
                   [--format terminal|json] [--bom PATH]
                   [--placement PATH] [--supply-snapshot PATH]
                   [--dfm-declarations LOCAL-JSON]
                   [--scope design|fabrication|assembly|full]
                   [--profile eurocircuits|aisler|jlcpcb|pcbway]
                   [--output FILE] [--preset standard|compact|relaxed]
                   [--fail-on critical|high|medium|low|info|never]
                   [--native auto|off|required] [--open]
  ratemypcb doctor
  ratemypcb profiles [list|show NAME]
  ratemypcb digest REPORT.json
  ratemypcb render --report FILE [--assessment FILE] --output FILE
  ratemypcb schema [--output FILE]
  ratemypcb version

The review never modifies or uploads PCB data. Native checks are optional.
--schematic resolves only ambiguous automatic roots inside the reviewed project.
Gerber/X2, Gerber Job 2023.06, strict XNC, and named KiCad/LibrePCB Excellon
profiles are parsed locally; official corpora remain local-only. ODB++ and
IPC-2581 are unsupported. Browser Gerber rendering is presentation-only.
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
    schematic: Option<String>,
    bom: Option<PathBuf>,
    placement: Option<PathBuf>,
    supply_snapshot: Option<PathBuf>,
    dfm_declarations: Option<PathBuf>,
    format: OutputFormat,
    output: Option<PathBuf>,
    preset: Preset,
    fail_on: Option<Severity>,
    native: NativeMode,
    open: bool,
    scope: ReviewScope,
    profile: Option<String>,
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
        schematic: None,
        bom: None,
        placement: None,
        supply_snapshot: None,
        dfm_declarations: None,
        format: OutputFormat::Terminal,
        output: None,
        preset: Preset::named("standard").unwrap(),
        fail_on: None,
        native: NativeMode::Auto,
        open: false,
        scope: ReviewScope::Full,
        profile: None,
    };
    let mut path_set = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--board" => parsed.board = Some(value(args, &mut i, "--board")?),
            "--schematic" => parsed.schematic = Some(value(args, &mut i, "--schematic")?),
            "--bom" => parsed.bom = Some(PathBuf::from(value(args, &mut i, "--bom")?)),
            "--placement" => {
                parsed.placement = Some(PathBuf::from(value(args, &mut i, "--placement")?))
            }
            "--supply-snapshot" => {
                parsed.supply_snapshot =
                    Some(PathBuf::from(value(args, &mut i, "--supply-snapshot")?))
            }
            "--dfm-declarations" => {
                parsed.dfm_declarations =
                    Some(PathBuf::from(value(args, &mut i, "--dfm-declarations")?))
            }
            "--scope" => {
                let scope = value(args, &mut i, "--scope")?;
                parsed.scope = ReviewScope::parse(&scope)
                    .ok_or_else(|| format!("Unsupported review scope: {scope}"))?;
            }
            "--profile" => parsed.profile = Some(value(args, &mut i, "--profile")?),
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
            CoverageStatus::Failed => ("×", "FAILED", RED),
            CoverageStatus::Unsupported => ("–", "UNSUPPORTED", DIM),
            CoverageStatus::Stale => ("!", "STALE", ORANGE),
            CoverageStatus::Unknown => ("?", "UNKNOWN", DIM),
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
    let dfm_declarations = match parsed.dfm_declarations.as_deref() {
        Some(path) => {
            let (source_file, source_path) = match dfm_declaration_source_path(path) {
                Ok(source) => source,
                Err(error) => {
                    eprintln!("ratemypcb: {error}");
                    return 2;
                }
            };
            let bytes =
                match read_bounded(&source_file, "DFM declarations", MAX_DFM_DECLARATION_BYTES) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        eprintln!("ratemypcb: {error}");
                        return 2;
                    }
                };
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs();
            match DfmDeclarations::from_json(&source_path, &bytes, now) {
                Ok(declarations) => Some(declarations),
                Err(error) => {
                    eprintln!("ratemypcb: {error}");
                    return 2;
                }
            }
        }
        None => None,
    };
    let report = match review(
        &parsed.path,
        ReviewOptions {
            board: parsed.board,
            schematic: parsed.schematic,
            bom: parsed.bom,
            placement: parsed.placement,
            supply_snapshot: parsed.supply_snapshot,
            dfm_declarations,
            preset: parsed.preset,
            native: parsed.native,
            tool_version: VERSION.into(),
            scope: parsed.scope,
            profile: parsed.profile,
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
        report.findings.iter().any(|finding| {
            finding.gate_impact == GateImpact::Blocking && finding.severity >= threshold
        })
    }) {
        1
    } else {
        0
    }
}

fn doctor(args: &[String]) -> i32 {
    let version = std::process::Command::new("kicad-cli")
        .arg("version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());
    let major = version.as_deref().and_then(|version| {
        version
            .split(|character: char| !character.is_ascii_digit())
            .find(|part| !part.is_empty())?
            .parse::<u32>()
            .ok()
    });
    let supported = major.is_some_and(|major| [8, 9, 10].contains(&major));
    let capabilities = serde_json::json!({
        "pcbDrc": { "native": supported, "requires": "local KiCad CLI 8, 9, or 10 and an intact project" },
        "schematicErc": { "native": supported, "requires": "local KiCad CLI 8, 9, or 10 and a bounded schematic root" },
        "coherentProjectParity": { "native": supported, "requires": "one matching board, schematic root, and .kicad_pro basename" },
        "manufacturing": {
            "gerberX2": { "semantic": true, "adapter": ratemypcb_core::fabrication::GERBER_ADAPTER_VERSION },
            "gerberJob": { "semantic": true, "subset": "2023.06" },
            "xnc": { "semantic": true, "profiles": ["strict-xnc", "kicad-legacy-excellon", "librepcb-legacy-excellon"] },
            "nativePackageReconciliation": true,
            "officialCorpus": "local-only",
            "browserGerberEvidence": false,
            "unsupportedFormats": ["ODB++", "IPC-2581"]
        },
        "limitations": {
            "zipNativeChecks": false,
            "altiumNativeChecks": false,
            "genericNetlistNativeChecks": false,
            "note": "ZIP, Altium .SchDoc, and generic netlist inputs remain inventory or explicit-field evidence only."
        }
    });
    if args == ["--json"] {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "tool": "ratemypcb", "version": VERSION,
                "reportSchemaVersion": ratemypcb_core::SCHEMA_VERSION,
                "assessmentSchemaVersion": ASSESSMENT_SCHEMA_VERSION,
                "kicadCli": {
                    "detected": version.is_some(), "version": version, "major": major,
                    "supported": supported, "supportedMajors": [8, 9, 10]
                },
                "capabilities": capabilities,
                "profiles": ["eurocircuits", "aisler", "jlcpcb", "pcbway"],
                "nexarCredentials": std::env::var_os("NEXAR_CLIENT_ID").is_some() && std::env::var_os("NEXAR_CLIENT_SECRET").is_some()
            }))
            .unwrap()
        );
    } else if args.is_empty() {
        println!(
            "RateMyPCB {VERSION}\nReport schema: {}\nKiCad CLI: {}\nDetected major: {} ({})\nSupported majors: 8, 9, 10\nNative PCB DRC: {}\nNative schematic ERC: {}\nCoherent-project parity: {}\nGerber/X2: bounded production semantics\nGerber Job: bounded 2023.06 subset\nXNC: strict plus named KiCad/LibrePCB legacy profiles\nNative/package reconciliation: capability-gated and symmetric\nOfficial fabrication corpora: local-only\nODB++ / IPC-2581: unsupported\nBrowser Gerber parsing: presentation-only\nZIP native checks: disabled\nAltium .SchDoc: inventory only; no native checks\nGeneric netlists: explicit fields only; no native checks",
            ratemypcb_core::SCHEMA_VERSION,
            version.as_deref().unwrap_or("not detected"),
            major.map_or_else(|| "unknown".into(), |major| major.to_string()),
            if supported {
                "supported"
            } else {
                "unsupported"
            },
            if supported {
                "available"
            } else {
                "not available"
            },
            if supported {
                "available"
            } else {
                "not available"
            },
            if supported {
                "available for coherent projects"
            } else {
                "not available"
            },
        );
    } else {
        eprintln!("ratemypcb: unknown doctor option {}", args[0]);
        return 2;
    }
    0
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn digest_command(args: &[String]) -> i32 {
    let Some(path) = args.first() else {
        eprintln!("ratemypcb: digest requires a report file");
        return 2;
    };
    match fs::read(path) {
        Ok(bytes) => {
            println!("{}", digest_bytes(&bytes));
            0
        }
        Err(error) => {
            eprintln!("ratemypcb: cannot read {path}: {error}");
            2
        }
    }
}

fn profiles(args: &[String]) -> i32 {
    let names = ["eurocircuits", "aisler", "jlcpcb", "pcbway"];
    if args.is_empty() || args == ["list"] {
        for name in names {
            println!("{name}");
        }
        return 0;
    }
    if args.len() == 2 && args[0] == "show" {
        if let Some((preset, profile)) = Preset::profile(&args[1]) {
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "id": profile.id, "name": profile.name, "sourceUrl": profile.source_url,
                "sourceRetrieved": profile.source_retrieved,
                "minimumsMm": { "track": preset.track, "viaDiameter": preset.via, "drill": preset.drill, "annularRing": preset.annular }
            })).unwrap());
            return 0;
        }
    }
    eprintln!(
        "ratemypcb: use profiles list or profiles show <{}>",
        names.join("|")
    );
    2
}

fn render_snapshot(args: &[String]) -> i32 {
    let (mut report_path, mut assessment_path, mut output) = (None, None, None);
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].clone();
        let target = match flag.as_str() {
            "--report" => &mut report_path,
            "--assessment" => &mut assessment_path,
            "--output" => &mut output,
            other => {
                eprintln!("ratemypcb: unknown render option {other}");
                return 2;
            }
        };
        match value(args, &mut i, &flag) {
            Ok(value) => *target = Some(PathBuf::from(value)),
            Err(error) => {
                eprintln!("ratemypcb: {error}");
                return 2;
            }
        }
        i += 1;
    }
    let (Some(report_path), Some(output)) = (report_path, output) else {
        eprintln!("ratemypcb: render requires --report FILE and --output FILE");
        return 2;
    };
    let report_bytes = match read_bounded(&report_path, "Report", MAX_REPORT_BYTES) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("ratemypcb: {error}");
            return 2;
        }
    };
    let report: ratemypcb_core::Report = match serde_json::from_slice(&report_bytes) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("ratemypcb: invalid report JSON: {error}");
            return 2;
        }
    };
    if let Err(error) = validate_report(&report) {
        eprintln!("ratemypcb: invalid report: {error}");
        return 2;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    if let Err(error) = validate_report_supply_retention(&report, now) {
        eprintln!("ratemypcb: invalid report: {error}");
        return 2;
    }
    let assessment: Option<Assessment> = match assessment_path {
        Some(path) => match read_bounded(&path, "Assessment", MAX_ASSESSMENT_BYTES)
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|error| error.to_string()))
        {
            Ok(value) => Some(value),
            Err(error) => {
                eprintln!(
                    "ratemypcb: cannot read assessment {}: {error}",
                    path.display()
                );
                return 2;
            }
        },
        None => None,
    };
    if let Some(assessment) = assessment.as_ref() {
        if assessment.report_digest != digest_bytes(&report_bytes) {
            eprintln!("ratemypcb: assessment reportDigest does not match the report file");
            return 2;
        }
        if let Err(error) = validate_assessment(&report, assessment) {
            eprintln!("ratemypcb: {error}");
            return 2;
        }
    }
    match viewer::snapshot(&report, assessment.as_ref())
        .and_then(|html| fs::write(&output, html).map_err(|error| error.to_string()))
    {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("ratemypcb: cannot write {}: {error}", output.display());
            3
        }
    }
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
        Some("doctor") => doctor(&args[1..]),
        Some("profiles") => profiles(&args[1..]),
        Some("digest") => digest_command(&args[1..]),
        Some("render") => render_snapshot(&args[1..]),
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
                schematic: None,
                bom: None,
                placement: None,
                supply_snapshot: None,
                dfm_declarations: None,
                preset: Preset::named("standard").unwrap(),
                native: NativeMode::Off,
                tool_version: "test".into(),
                scope: ReviewScope::Full,
                profile: None,
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
    fn parses_explicit_bom() {
        let args = parse_review(&[
            "board.kicad_pcb".into(),
            "--bom".into(),
            "assembly.csv".into(),
        ])
        .unwrap();
        assert_eq!(args.bom.as_deref(), Some(Path::new("assembly.csv")));
    }

    #[test]
    fn dfm_declaration_source_paths_distinguish_external_namesakes() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ratemypcb-dfm-source-path-{nonce}"));
        let first = root.join("first/declarations.json");
        let second = root.join("second/declarations.json");
        fs::create_dir_all(first.parent().unwrap()).unwrap();
        fs::create_dir_all(second.parent().unwrap()).unwrap();
        fs::write(&first, b"{}").unwrap();
        fs::write(&second, b"{}").unwrap();
        let first = dfm_declaration_source_path(&first).unwrap().1;
        let second = dfm_declaration_source_path(&second).unwrap().1;
        assert_ne!(first, second);
        assert!(first.starts_with("external/") && first.ends_with("/declarations.json"));
        assert!(second.starts_with("external/") && second.ends_with("/declarations.json"));
        fs::remove_dir_all(root).unwrap();
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

    #[test]
    fn render_snapshot_rejects_invalid_report_before_rendering() {
        let mut report = fixture_report();
        report.evidence[1].id = report.evidence[0].id.clone();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ratemypcb-render-boundary-{nonce}"));
        fs::create_dir(&root).unwrap();
        let report_path = root.join("report.json");
        let output = root.join("report.html");
        fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
        assert_eq!(
            render_snapshot(&[
                "--report".into(),
                report_path.to_string_lossy().into_owned(),
                "--output".into(),
                output.to_string_lossy().into_owned(),
            ]),
            2
        );
        assert!(!output.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
