use ratemypcb_core::{Assessment, Report};
use serde_json::{Value, json};
use std::fs::File;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use zip::ZipArchive;

const INDEX: &[u8] = include_bytes!("../assets/local-viewer.html");
const STYLE: &[u8] = include_bytes!("../assets/local-viewer.css");
const APP: &[u8] = include_bytes!("../assets/local-viewer.js");
const BOARD_VIEW: &[u8] = include_bytes!("../assets/board-view.js");
const REPORT_VIEW_MODEL: &[u8] = include_bytes!("../assets/report-view-model.mjs");
const MAX_BOARD_BYTES: usize = 64 * 1024 * 1024;
const MAX_GERBER_BYTES: usize = 8 * 1024 * 1024;
const MAX_GERBER_TOTAL_BYTES: usize = 20 * 1024 * 1024;
const MAX_GERBERS: usize = 20;

fn read_limited(reader: impl Read, limit: usize, label: &str) -> Result<String, String> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read {label}: {error}"))?;
    if bytes.len() > limit {
        return Err(format!(
            "{label} exceeds the local viewer's {limit}-byte limit."
        ));
    }
    String::from_utf8(bytes).map_err(|_| format!("{label} is not UTF-8 text."))
}

fn selected_board(report: &Report) -> Option<&str> {
    report.input.selected_board.as_deref().or_else(|| {
        report
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == "board" && artifact.selected)
            .map(|artifact| artifact.path.as_str())
    })
}

fn wanted_gerbers(report: &Report) -> Vec<&str> {
    report
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == "gerber")
        .take(MAX_GERBERS)
        .map(|artifact| artifact.path.as_str())
        .collect()
}

fn push_gerber(
    gerbers: &mut Vec<Value>,
    failures: &mut Vec<String>,
    total: &mut usize,
    path: &str,
    source: Result<String, String>,
) {
    match source {
        Ok(source) if *total + source.len() <= MAX_GERBER_TOTAL_BYTES => {
            *total += source.len();
            gerbers.push(json!({ "path": path, "source": source }));
        }
        Ok(_) => failures.push(format!(
            "{path} was omitted because the local viewer's 20 MB Gerber budget was reached."
        )),
        Err(error) => failures.push(error),
    }
}

fn payload_from_zip(
    path: &Path,
    report: &Report,
    assessment: Option<&Assessment>,
) -> Result<Value, String> {
    let file =
        File::open(path).map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).map_err(|_| "Cannot reopen the reviewed ZIP.".to_string())?;
    let board_path = selected_board(report).map(str::to_owned);
    let board = if let Some(name) = board_path {
        let entry = archive
            .by_name(&name)
            .map_err(|_| format!("The selected board {name} is no longer present in the ZIP."))?;
        Some(
            json!({ "path": name, "source": read_limited(entry, MAX_BOARD_BYTES, "selected board")? }),
        )
    } else {
        None
    };

    let mut gerbers = Vec::new();
    let mut failures = Vec::new();
    let mut total = 0;
    for name in wanted_gerbers(report) {
        let source = archive
            .by_name(name)
            .map_err(|_| format!("{name} is no longer present in the ZIP."))
            .and_then(|entry| read_limited(entry, MAX_GERBER_BYTES, name));
        push_gerber(&mut gerbers, &mut failures, &mut total, name, source);
    }
    Ok(
        json!({ "report": report, "assessment": assessment, "board": board, "gerbers": gerbers, "failures": failures }),
    )
}

fn payload_from_filesystem(
    path: &Path,
    report: &Report,
    assessment: Option<&Assessment>,
) -> Result<Value, String> {
    let root = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or_else(|| Path::new("."))
    };
    let board = if path.is_file()
        && path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("kicad_pcb"))
    {
        Some(json!({
            "path": path.file_name().and_then(|value| value.to_str()).unwrap_or("board.kicad_pcb"),
            "source": read_limited(File::open(path).map_err(|error| error.to_string())?, MAX_BOARD_BYTES, "selected board")?
        }))
    } else if let Some(name) = selected_board(report) {
        Some(json!({
            "path": name,
            "source": read_limited(File::open(root.join(name)).map_err(|error| format!("Cannot open {name}: {error}"))?, MAX_BOARD_BYTES, name)?
        }))
    } else {
        None
    };

    let mut gerbers = Vec::new();
    let mut failures = Vec::new();
    let mut total = 0;
    for name in wanted_gerbers(report) {
        let source = File::open(root.join(name))
            .map_err(|error| format!("Cannot open {name}: {error}"))
            .and_then(|file| read_limited(file, MAX_GERBER_BYTES, name));
        push_gerber(&mut gerbers, &mut failures, &mut total, name, source);
    }
    Ok(
        json!({ "report": report, "assessment": assessment, "board": board, "gerbers": gerbers, "failures": failures }),
    )
}

fn viewer_payload(
    path: &Path,
    report: &Report,
    assessment: Option<&Assessment>,
) -> Result<Vec<u8>, String> {
    let value = if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        payload_from_zip(path, report, assessment)?
    } else {
        payload_from_filesystem(path, report, assessment)?
    };
    serde_json::to_vec(&value)
        .map_err(|error| format!("Cannot encode local viewer session: {error}"))
}

fn capability_token() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| format!("Cannot create viewer token: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn launch_browser(url: &str) -> Result<(), String> {
    if std::env::var_os("RATEMYPCB_NO_BROWSER").is_some() {
        return Ok(());
    }
    let mut command = if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(url);
        command
    } else if cfg!(target_os = "windows") {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Cannot launch the browser: {error}. Open {url} manually."))
}

fn response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    let headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'\r\nCross-Origin-Resource-Policy: same-origin\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(headers.as_bytes())
        .and_then(|_| stream.write_all(body))
        .map_err(|error| format!("Cannot serve local viewer: {error}"))
}

fn handle(mut stream: TcpStream, port: u16, token: &str, payload: &[u8]) -> Result<bool, String> {
    stream
        .set_nonblocking(false)
        .map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| error.to_string())?;
    let mut request = [0_u8; 16 * 1024];
    let read = stream
        .read(&mut request)
        .map_err(|error| format!("Cannot read viewer request: {error}"))?;
    let request = String::from_utf8_lossy(&request[..read]);
    let mut lines = request.split("\r\n");
    let first = lines.next().unwrap_or_default();
    let host = lines.clone().find_map(|line| {
        line.strip_prefix("Host: ")
            .or_else(|| line.strip_prefix("host: "))
    });
    if host != Some(&format!("127.0.0.1:{port}")) {
        response(
            &mut stream,
            "421 Misdirected Request",
            "text/plain; charset=utf-8",
            b"Invalid local viewer host.",
        )?;
        return Ok(false);
    }
    let path = first
        .strip_prefix("GET ")
        .and_then(|rest| rest.split_whitespace().next());
    let (status, content_type, body, served_session): (&str, &str, &[u8], bool) = match path {
        Some("/") => ("200 OK", "text/html; charset=utf-8", INDEX, false),
        Some("/local-viewer.css") => ("200 OK", "text/css; charset=utf-8", STYLE, false),
        Some("/local-viewer.js") => ("200 OK", "text/javascript; charset=utf-8", APP, false),
        Some("/board-view.js") => (
            "200 OK",
            "text/javascript; charset=utf-8",
            BOARD_VIEW,
            false,
        ),
        Some("/report-view-model.mjs") => (
            "200 OK",
            "text/javascript; charset=utf-8",
            REPORT_VIEW_MODEL,
            false,
        ),
        Some("/session") => {
            let supplied = lines.find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("x-ratemypcb-token")
                    .then(|| value.trim())
            });
            if supplied == Some(token) {
                ("200 OK", "application/json; charset=utf-8", payload, true)
            } else {
                (
                    "403 Forbidden",
                    "text/plain; charset=utf-8",
                    b"Invalid local viewer capability.",
                    false,
                )
            }
        }
        _ => (
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"Not found.",
            false,
        ),
    };
    response(&mut stream, status, content_type, body)?;
    Ok(served_session)
}

pub fn open(path: &Path, report: &Report) -> Result<(), String> {
    let payload = viewer_payload(path, report, None)?;
    let token = capability_token()?;
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("Cannot bind the loopback viewer: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let url = format!("http://127.0.0.1:{port}/#{token}");
    eprintln!("Local viewer: {url}\nPCB data will stay on this computer.");
    launch_browser(&url)?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(300);
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                if handle(stream, port, &token, &payload)? {
                    return Ok(());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(format!("Local viewer stopped: {error}")),
        }
    }
    Err("Timed out waiting for the browser to load the local review.".into())
}

pub fn snapshot(report: &Report, assessment: Option<&Assessment>) -> Result<String, String> {
    // A saved report is untrusted metadata, not authority to reopen local design files.
    let payload = serde_json::to_vec(&json!({
        "report": report,
        "assessment": assessment,
        "board": null,
        "gerbers": [],
        "failures": []
    }))
    .expect("serializing a report cannot fail");
    let payload = String::from_utf8(payload)
        .map_err(|_| "Cannot encode snapshot payload.".to_string())?
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    let html = std::str::from_utf8(INDEX).map_err(|error| error.to_string())?;
    let title = assessment
        .map(|assessment| {
            format!(
                "{} — {}",
                assessment.disposition.to_uppercase(),
                assessment.verdict
            )
        })
        .unwrap_or_else(|| "UNASSESSED — RateMyPCB manufacturing release review".into())
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let html = html
        .replace(
            "<title>RateMyPCB manufacturing release review</title>",
            &format!("<title>{title}</title>"),
        )
        .replace(
            "    <link rel=\"stylesheet\" href=\"/local-viewer.css\" />",
            &format!("    <style>{}</style>", String::from_utf8_lossy(STYLE)),
        )
        .replace(
            "    <script type=\"module\" src=\"/local-viewer.js\"></script>",
            "__RATEMYPCB_SCRIPTS__",
        )
        .replace(
            "LOCAL SESSION · NOTHING UPLOADED",
            "OFFLINE SNAPSHOT · NOTHING UPLOADED",
        )
        .replace(
            "This page and its PCB data were served by the RateMyPCB process on your computer.",
            "This self-contained report keeps its PCB data in this HTML file.",
        );
    let board = String::from_utf8_lossy(BOARD_VIEW).replace("export ", "");
    let model = String::from_utf8_lossy(REPORT_VIEW_MODEL).replace("export ", "");
    let app = String::from_utf8_lossy(APP)
        .replace(
            "import {\n  combineGerbers,\n  createBoardViewer,\n  parseGerber,\n  parseKiCadView,\n} from \"/board-view.js\";\n",
            "",
        )
        .replace(
            "import {\n  filterAndSortBomLines,\n  normalizedStatus,\n  schematicEvidenceRefs,\n  supplyDetailGroups,\n} from \"/report-view-model.mjs\";\n",
            "",
        );
    Ok(html.replace(
        "__RATEMYPCB_SCRIPTS__",
        &format!("  <script>globalThis.RATEMYPCB_PAYLOAD={payload};</script>\n  <script>{board}\n{model}\n{app}</script>"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    #[test]
    fn capability_tokens_are_random_and_256_bit_hex() {
        let first = capability_token().unwrap();
        let second = capability_token().unwrap();
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|character| character.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }

    #[test]
    fn embedded_viewer_has_no_cloud_api_calls() {
        let app = std::str::from_utf8(APP).unwrap();
        assert!(!app.contains("/api/"));
        assert!(app.contains("fetch(\"/session\""));
        assert!(app.contains("function evidenceAnchor(publicId)"));
        assert!(app.contains("link.href = `#${evidenceAnchor(publicId)}`"));
        assert!(app.contains("node.id = evidenceAnchor(record.id)"));
        assert!(app.contains("function renderBom(report)"));
        assert!(app.contains("let bomVisible = 100"));
        assert!(app.contains("let evidenceVisible = 100"));
        assert!(app.contains("navigator.clipboard?.writeText"));
        assert!(app.contains("event.key === \"Home\""));
        assert!(app.contains("event.key === \"End\""));
        let model = std::str::from_utf8(REPORT_VIEW_MODEL).unwrap();
        assert!(model.contains("line.releaseImpact.status"));
        assert!(model.contains("function filterAndSortBomLines"));
        for policy in [
            "checksRating",
            "categoryRating",
            "bomLineRating",
            "combinedBomJudgment",
            "No findings",
        ] {
            assert!(
                !app.contains(policy),
                "viewer must not own policy: {policy}"
            );
        }
        let html = std::str::from_utf8(INDEX).unwrap();
        let decision = html
            .find("data-report-landmark=\"release-decision\"")
            .unwrap();
        let completeness = html.find("data-report-landmark=\"completeness\"").unwrap();
        let scores = html.find("data-report-landmark=\"scores\"").unwrap();
        assert!(decision < completeness && completeness < scores);
        assert_eq!(
            html.matches("data-report-landmark=\"release-decision\"")
                .count(),
            1
        );
        assert!(html.contains("role=\"tablist\""));
        assert!(html.contains("id=\"bom-panel\""));
        assert!(html.contains("id=\"bom-rows\""));
        assert!(html.contains("Sourceability"));
        assert!(html.contains("Lifecycle"));
        assert!(html.contains("Alternatives"));
        assert!(html.contains("Release impact"));
        assert!(html.contains("<caption>Bill of materials release-impact risk matrix</caption>"));
        assert!(html.contains("aria-describedby=\"viewer-fallback\""));
        let style = std::str::from_utf8(STYLE).unwrap();
        assert!(style.contains(":focus-visible"));
        assert!(style.contains("@media print"));
    }

    #[test]
    fn zip_payload_contains_reviewed_board_and_gerbers() {
        let path = std::env::temp_dir().join(format!(
            "ratemypcb-viewer-{}-{}.zip",
            std::process::id(),
            capability_token().unwrap()
        ));
        let file = File::create(&path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        archive
            .start_file("main.kicad_pcb", SimpleFileOptions::default())
            .unwrap();
        archive
            .write_all(b"(kicad_pcb (version 20240108))")
            .unwrap();
        archive
            .start_file("main-F_Cu.gbr", SimpleFileOptions::default())
            .unwrap();
        archive
            .write_all(b"%FSLAX24Y24*%\n%MOMM*%\nG01*\nM02*")
            .unwrap();
        archive.finish().unwrap();

        let report: Report = serde_json::from_value(json!({
            "schemaVersion": "2.0",
            "tool": { "name": "ratemypcb", "version": "test" },
            "input": { "path": path.display().to_string(), "kind": "fabrication-zip", "selectedBoard": "main.kicad_pcb" },
            "artifacts": [
                { "path": "main.kicad_pcb", "kind": "board", "format": "kicad", "selected": true },
                { "path": "main-F_Cu.gbr", "kind": "gerber", "format": "rs-274x", "selected": false }
            ],
            "score": { "value": 10.0, "raw": 100, "verdict": "test" },
            "observedRisk": { "scoreRaw": 100, "highestSeverity": null },
            "confidence": "medium",
            "evidenceConfidence": "unknown",
            "freshness": "unknown",
            "requiredEvidence": [],
            "evidence": [],
            "coverage": [],
            "findings": [],
            "nativeDrc": { "status": "not-run", "tool": "kicad-cli", "version": null, "findingCount": 0, "note": "test", "violations": [] },
            "profileDrc": null,
            "reviewScope": "full",
            "categories": [],
            "approvalEligible": false,
            "profile": null,
            "bom": { "status": "not-provided", "lineCount": 0, "lines": [] },
            "stackup": null,
            "limitations": [],
            "disclaimer": "test"
        }))
        .unwrap();
        let payload: Value =
            serde_json::from_slice(&viewer_payload(&path, &report, None).unwrap()).unwrap();
        assert_eq!(payload["board"]["path"], "main.kicad_pcb");
        assert!(
            payload["board"]["source"]
                .as_str()
                .unwrap()
                .starts_with("(kicad_pcb")
        );
        assert_eq!(payload["gerbers"][0]["path"], "main-F_Cu.gbr");
        let snapshot = snapshot(&report, None).unwrap();
        assert!(!snapshot.contains("(kicad_pcb (version 20240108))"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn synthetic_large_layer_fits_the_viewer_file_limit() {
        let layer = vec![b'X'; 4 * 1024 * 1024 + 1];
        assert!(layer.len() > 4 * 1024 * 1024);
        assert!(read_limited(std::io::Cursor::new(layer), MAX_GERBER_BYTES, "board.gbo").is_ok());
    }
}
