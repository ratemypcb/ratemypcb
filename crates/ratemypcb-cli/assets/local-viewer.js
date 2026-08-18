import { combineGerbers, createBoardViewer, parseGerber, parseKiCadView } from "/board-view.js";

const $ = (selector) => document.querySelector(selector);
const token = decodeURIComponent(location.hash.slice(1));
history.replaceState({}, "", location.pathname);
const viewer = createBoardViewer($("#board-canvas"));
let boardModel = null;
let gerberModel = null;
let active = "board";

function entry(title, evidence, tag, tagClass = "") {
  const node = document.createElement("div");
  node.className = "entry";
  const heading = document.createElement("strong");
  const badge = document.createElement("span");
  badge.className = `tag ${tagClass}`;
  badge.textContent = tag;
  heading.append(badge, document.createTextNode(title));
  const copy = document.createElement("p");
  copy.textContent = evidence;
  node.append(heading, copy);
  return node;
}

function renderLayers(model) {
  const root = $("#layers");
  root.replaceChildren();
  for (const layer of model?.layers || []) {
    const label = document.createElement("label");
    label.className = "layer";
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = layer.visible;
    checkbox.addEventListener("change", () => viewer.setLayerVisible(layer.id, checkbox.checked));
    const dot = document.createElement("i");
    dot.style.setProperty("--layer-color", layer.color);
    const copy = document.createElement("span");
    copy.textContent = layer.name || layer.id;
    const count = document.createElement("small");
    count.textContent = `${(layer.strokes?.length || 0) + (layer.flashes?.length || 0) + (layer.polygons?.length || 0)} drawable items`;
    copy.append(count);
    label.append(checkbox, dot, copy);
    root.append(label);
  }
  if (!model) root.textContent = "No viewable layers were included in this review.";
}

function show(kind) {
  active = kind;
  const model = kind === "gerber" ? gerberModel : boardModel;
  viewer.setModel(model);
  renderLayers(model);
  $("#board-tab").classList.toggle("active", kind === "board");
  $("#gerber-tab").classList.toggle("active", kind === "gerber");
  $("#viewer-note").textContent = model?.warnings?.join(" ") || (model ? `${model.primitives} supported primitives rendered locally.` : "No local geometry is available for this view.");
}

function renderReport(payload) {
  const report = payload.report;
  $("#verdict").textContent = report.score.verdict;
  $("#score").textContent = Number(report.score.value).toFixed(1);
  $("#source").textContent = `${report.input.kind} · ${report.input.path} · ${report.confidence} confidence`;
  $("#coverage").replaceChildren(...report.coverage.map((item) => entry(item.label, item.evidence, item.status)));
  $("#findings").replaceChildren(...(report.findings.length
    ? report.findings.map((item) => entry(item.title, `${item.evidence} Fix: ${item.recommendation}`, item.severity, item.severity))
    : [entry("No deterministic findings", "No findings were produced by the checks that ran.", "pass")]));
  const limitations = $("#limitations");
  limitations.replaceChildren(...report.limitations.map((text) => {
    const item = document.createElement("li");
    item.textContent = text;
    return item;
  }));
  $("#disclaimer").textContent = report.disclaimer;
}

async function load() {
  if (!token) throw new Error("The local viewer capability token is missing.");
  const response = await fetch("/session", { headers: { "x-ratemypcb-token": token }, cache: "no-store" });
  if (!response.ok) throw new Error(`The local viewer session is unavailable (${response.status}).`);
  const payload = await response.json();
  renderReport(payload);
  if (payload.board?.source) boardModel = parseKiCadView(payload.board.source);
  const gerbers = [];
  let budget = 100000;
  const failures = [...(payload.failures || [])];
  for (const file of payload.gerbers || []) {
    try {
      const model = parseGerber(file.source, file.path, { maxPrimitives: budget });
      budget -= model.primitives;
      gerbers.push(model);
    } catch (error) {
      failures.push(`${file.path}: ${error.message}`);
    }
  }
  if (gerbers.length) {
    gerberModel = combineGerbers(gerbers);
    gerberModel.warnings.push(...failures);
  }
  $("#board-tab").disabled = !boardModel;
  $("#gerber-tab").disabled = !gerberModel;
  show(boardModel ? "board" : "gerber");
}

$("#board-tab").addEventListener("click", () => show("board"));
$("#gerber-tab").addEventListener("click", () => show("gerber"));
$("#fit").addEventListener("click", () => viewer.fit());
$("#zoom-in").addEventListener("click", () => viewer.zoom(1.2));
$("#zoom-out").addEventListener("click", () => viewer.zoom(1 / 1.2));
load().catch((error) => {
  $("#verdict").textContent = "Local viewer unavailable";
  $("#source").textContent = error.message;
  $("#source").classList.add("error");
});
