import {
  combineGerbers,
  createBoardViewer,
  parseGerber,
  parseKiCadView,
} from "/board-view.js";
import {
  filterAndSortBomLines,
  normalizedStatus,
  schematicEvidenceRefs,
  supplyDetailGroups,
} from "/report-view-model.mjs";

const $ = (selector) => document.querySelector(selector);
const initialHash = location.hash.slice(1);
const token = /^[0-9a-f]{64}$/.test(initialHash) ? initialHash : "";
if (token) history.replaceState({}, "", location.pathname);
const viewer = createBoardViewer($("#board-canvas"));
let boardModel = null;
let gerberModel = null;
let claimSequence = 0;
let bomLines = [];
let bomVisible = 100;
let evidenceRecords = [];
let evidenceVisible = 100;
let evidenceDetails = new Map();
let renderedEvidenceIds = new Set();
let categoryRenderers = [];

function evidenceAnchor(publicId) {
  return `evidence-${encodeURIComponent(publicId)}`;
}

function evidenceLink(publicId) {
  const link = document.createElement("a");
  link.className = "evidence-ref";
  link.id = `claim-${++claimSequence}`;
  link.href = `#${evidenceAnchor(publicId)}`;
  link.textContent = publicId;
  link.addEventListener("click", () => {
    reportTab("overview");
    const target = ensureEvidenceRecord(publicId);
    if (!target) return;
    target.querySelector(".evidence-back").href = `#${link.id}`;
    requestAnimationFrame(() => target.focus());
  });
  return link;
}

async function copyEvidenceId(publicId, button) {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(publicId);
    } else {
      const input = document.createElement("textarea");
      input.value = publicId;
      input.style.cssText = "position:fixed;opacity:0";
      document.body.append(input);
      input.select();
      document.execCommand("copy");
      input.remove();
    }
    button.textContent = "Copied ID";
  } catch {
    button.textContent = "Copy unavailable";
  }
}

function renderRefs(root, refs) {
  root.replaceChildren(...refs.map(evidenceLink));
}

function entry(title, evidence, tag, refs = []) {
  const node = document.createElement("div");
  node.className = "entry";
  const heading = document.createElement("strong");
  if (tag != null) {
    const badge = document.createElement("span");
    badge.className = `tag ${normalizedStatus(tag)}`;
    badge.textContent = normalizedStatus(tag).replaceAll("-", " ");
    heading.append(badge);
  }
  heading.append(document.createTextNode(title));
  node.append(heading);
  if (evidence != null) {
    const copy = document.createElement("p");
    copy.textContent = evidence;
    node.append(copy);
  }
  if (refs.length) {
    const links = document.createElement("div");
    links.className = "evidence-refs";
    renderRefs(links, refs);
    node.append(links);
  }
  return node;
}

function reportTab(name) {
  const overview = name === "overview";
  $("#overview-panel").hidden = !overview;
  $("#bom-panel").hidden = overview;
  for (const [tab, active] of [
    [$("#overview-tab"), overview],
    [$("#bom-tab"), !overview],
  ]) {
    tab.classList.toggle("active", active);
    tab.setAttribute("aria-selected", String(active));
    tab.tabIndex = active ? 0 : -1;
  }
}

function textCell(label, primary, secondary, status) {
  const cell = document.createElement("td");
  cell.dataset.label = label;
  if (status != null) {
    const badge = document.createElement("span");
    badge.className = `tag ${normalizedStatus(status)}`;
    badge.textContent = normalizedStatus(status).replaceAll("-", " ");
    cell.append(badge);
  }
  const main = document.createElement("strong");
  main.textContent = primary == null ? "—" : String(primary);
  cell.append(main);
  if (secondary != null) {
    const note = document.createElement("small");
    note.textContent = String(secondary);
    cell.append(note);
  }
  return cell;
}

function judgmentCell(label, judgment) {
  return textCell(label, judgment?.detail, null, judgment?.status);
}

function sourceabilityCell(line) {
  const cell = textCell("Sourceability", line.sourceability.detail, null, line.sourceability.status);
  for (const [title, detailLines] of supplyDetailGroups(line)) {
    const details = document.createElement("details");
    const summary = document.createElement("summary");
    summary.textContent = `${title} (${detailLines.length})`;
    details.append(summary);
    for (const text of detailLines) {
      const item = document.createElement("small");
      item.textContent = text;
      details.append(item);
    }
    cell.append(details);
  }
  return cell;
}

function bomRow(line) {
  const row = document.createElement("tr");
  const refs = line.references.join(", ");
  const identity = [line.manufacturer, line.mpn]
    .filter((value) => value != null)
    .join(" · ");
  row.append(
    textCell(
      "Refs / value",
      refs,
      [line.value, line.quantity == null ? null : `Qty ${line.quantity}`, line.footprint]
        .filter((value) => value != null)
        .join(" · "),
    ),
    textCell("Identity", identity, line.identity.detail, line.identity.status),
    sourceabilityCell(line),
    judgmentCell("Lifecycle", line.lifecycle),
    textCell("Stock", line.stock, line.requiredQuantity == null ? (line.moq == null ? null : `MOQ ${line.moq}`) : `Required ${line.requiredQuantity} · MOQ ${line.moq ?? "unknown"}`, line.sourceability.status),
    textCell("Price", line.unitPriceDecimal ?? line.unitPrice, line.currency, line.pricing.status),
    judgmentCell("Alternatives", line.alternatives),
    judgmentCell("Release impact", line.releaseImpact),
  );
  return row;
}

function updateBomRows() {
  const query = $("#bom-search").value.trim().toLowerCase();
  const filter = $("#bom-filter").value;
  const sort = $("#bom-sort").value;
  const matches = filterAndSortBomLines(bomLines, query, filter, sort);
  const shown = matches.slice(0, bomVisible);
  const rows = shown.map(({ line }) => bomRow(line));
  if (!rows.length) {
    const row = document.createElement("tr");
    row.className = "bom-empty";
    const cell = document.createElement("td");
    cell.colSpan = 8;
    cell.textContent = bomLines.length ? "No BOM lines match these controls." : "No BOM was provided.";
    row.append(cell);
    rows.push(row);
  }
  $("#bom-rows").replaceChildren(...rows);
  $("#bom-results").textContent = `Showing ${shown.length} of ${matches.length} matching lines (${bomLines.length} total).`;
  $("#bom-more").hidden = shown.length >= matches.length;
}

function renderBom(report) {
  const bom = report.bom;
  bomLines = bom.lines;
  bomVisible = 100;
  $("#bom-status").textContent = bom.status;
  $("#bom-lines").textContent = String(bom.lineCount);
  $("#bom-tab-count").textContent = bomLines.length ? `(${bomLines.length})` : "";
  $("#bom-note").textContent = `${bom.lineCount} BOM line${bom.lineCount === 1 ? "" : "s"}; core status: ${bom.status}.`;
  updateBomRows();
}

function evidenceFreshness(report) {
  const observedTimes = report.evidence
    .map((item) => item.provenance.observedAt)
    .filter((value) => /^\d{4}-\d{2}-\d{2}T/.test(value || ""));
  const observed = observedTimes.length
    ? `latest observed ${observedTimes.sort().at(-1)}`
    : "observed time not provided";
  return `${normalizedStatus(report.freshness)}; ${observed}`;
}

function renderRequiredEvidence(report) {
  const evidence = new Map(report.evidence.map((record) => [record.id, record]));
  const counts = { completed: 0, attention: 0, "not-run": 0, "not-provided": 0 };
  for (const item of report.requiredEvidence) {
    if (item.execution === "not_provided") counts["not-provided"] += 1;
    else if (item.execution !== "completed") counts["not-run"] += 1;
    else if (item.result === "pass" && !["stale", "unknown"].includes(item.freshness)) counts.completed += 1;
    else counts.attention += 1;
  }
  const summary = [
    `completed: ${counts.completed}`,
    `attention: ${counts.attention}`,
    `not run: ${counts["not-run"]}`,
    `not provided: ${counts["not-provided"]}`,
    `overall freshness: ${normalizedStatus(report.freshness)}`,
  ];
  $("#completeness-summary").replaceChildren(
    ...summary.map((text) => Object.assign(document.createElement("span"), { textContent: text })),
  );
  $("#required-evidence").replaceChildren(
    ...report.requiredEvidence.map((item) => {
      const producer = evidence.get(item.evidenceId)?.provenance?.producer;
      const status = item.execution === "completed" && item.result !== "pass" ? "attention" : item.execution;
      return entry(
        item.checkId,
        `execution: ${item.execution}; result: ${item.result}; freshness: ${item.freshness}; confidence: ${item.confidence}; source: ${producer ? `${producer.name} ${producer.version}` : "not provided"}`,
        status,
        [item.evidenceId],
      );
    }),
  );
}

function evidenceArticle(record) {
  const detail = evidenceDetails.get(record.id);
  const provenance = record.provenance;
  const node = document.createElement("article");
  node.className = "evidence-detail";
  node.id = evidenceAnchor(record.id);
  node.tabIndex = -1;
  const heading = document.createElement("h3");
  heading.textContent = record.id;
  const controls = document.createElement("div");
  controls.className = "evidence-controls";
  const copy = document.createElement("button");
  copy.type = "button";
  copy.textContent = "Copy evidence ID";
  copy.addEventListener("click", () => copyEvidenceId(record.id, copy));
  const back = document.createElement("a");
  back.className = "evidence-back";
  back.href = "#evidence-details-heading";
  back.textContent = "Return to claim";
  controls.append(copy, back);
  const summary = document.createElement("p");
  summary.textContent = detail?.evidence || "No evidence summary was provided.";
  const values = [
    ["Check ID", record.checkId],
    ["Kind", record.kind],
    ["Artifact ID", provenance.artifactId],
    ["Artifact digest", provenance.artifactDigest],
    [
      "Producer",
      `${provenance.producer.kind} · ${provenance.producer.name} · ${provenance.producer.version}`,
    ],
    ["Location", JSON.stringify(provenance.location)],
    ["Evidence class", provenance.evidenceClass],
    ["Confidence", provenance.confidence],
    ["Freshness", provenance.freshness],
    ["Observed at", provenance.observedAt ?? "Not provided"],
  ];
  const list = document.createElement("dl");
  for (const [label, value] of values) {
    const term = document.createElement("dt");
    term.textContent = label;
    const description = document.createElement("dd");
    description.textContent = String(value);
    list.append(term, description);
  }
  node.append(heading, controls, summary, list);
  return node;
}

function appendEvidenceRecords(records) {
  const root = $("#evidence-records");
  for (const record of records) {
    if (renderedEvidenceIds.has(record.id)) continue;
    renderedEvidenceIds.add(record.id);
    root.append(evidenceArticle(record));
  }
  $("#evidence-results").textContent =
    `Showing ${renderedEvidenceIds.size} of ${evidenceRecords.length} evidence records.`;
  $("#evidence-more").hidden = renderedEvidenceIds.size >= evidenceRecords.length;
}

function ensureEvidenceRecord(publicId) {
  const existing = document.getElementById(evidenceAnchor(publicId));
  if (existing) return existing;
  const record = evidenceRecords.find((item) => item.id === publicId);
  if (!record) return null;
  appendEvidenceRecords([record]);
  return document.getElementById(evidenceAnchor(publicId));
}

function renderSchematicEvidence(report) {
  const schematic = report.schematic;
  if (!schematic || schematic.status === "not_provided") return;
  const node = document.createElement("details");
  node.className = "category";
  const heading = document.createElement("summary");
  heading.textContent = `Schematic · ${schematic.status} · ${schematic.occurrenceCount} occurrence(s)`;
  node.append(heading);
  const refs = schematicEvidenceRefs(report);
  const pair = schematic.sourcePair;
  node.append(entry(
    "Project source pair",
    pair
      ? `project: ${pair.projectIdentity}; schematic: ${pair.schematicPath} (${pair.schematicDigest}); board: ${pair.boardPath} (${pair.boardDigest})`
      : `project: ${schematic.projectIdentity ?? "not provided"}; coherent source pair: not available`,
    schematic.status,
    refs,
  ));
  for (const capability of schematic.capabilities) {
    node.append(entry(
      capability.id,
      `status: ${capability.status}; producer: ${capability.producer}; evidence class: ${capability.evidenceClass}; ${capability.detail}`,
      capability.status,
      refs,
    ));
  }
  for (const mismatch of schematic.mismatches) {
    const finding = report.findings.find((item) => item.location === mismatch.location);
    node.append(entry(
      `${mismatch.checkId} · ${mismatch.gateImpact}`,
      `sheet/item: ${mismatch.location}; source pair value: ${mismatch.expected} → ${mismatch.actual}; join: ${mismatch.join}; confidence: ${mismatch.confidence}`,
      "attention",
      finding ? [finding.id] : refs,
    ));
  }
  for (const [channel, native, checkId] of [
    ["ERC", schematic.nativeErc, "schematic-erc"],
    ["schematic parity", schematic.nativeParity, "schematic-parity"],
  ]) {
    if (!native) continue;
    const nativeRefs = schematicEvidenceRefs(report, checkId);
    node.append(entry(
      `${channel} · ${native.status}`,
      `producer: ${native.tool} ${native.version ?? "not observed"}; report version: ${native.reportVersion ?? "not observed"}; active: ${native.findingCount}; excluded: ${native.excludedCount}; exclusion unknown: ${native.unknownExclusionCount}; ${native.note}`,
      native.status,
      nativeRefs,
    ));
    for (const marker of native.violations) {
      node.append(entry(
        `${marker.group} · ${marker.violationType}`,
        `sheet: ${marker.sheetPath ?? "not provided"}; UUID path: ${marker.sheetUuidPath ?? "root"}; location: ${marker.structuralLocation}; exclusion: ${marker.excluded ?? "unknown"}; ${marker.description}${marker.comment ? `; comment: ${marker.comment}` : ""}`,
        marker.severity,
        nativeRefs,
      ));
    }
  }
  $("#categories").append(node);
}

function renderEvidenceRecords(report) {
  evidenceRecords = report.evidence;
  evidenceVisible = 100;
  evidenceDetails = new Map([
    ...report.findings.map((item) => [item.id, item]),
    ...report.coverage.map((item) => [item.id, item]),
  ]);
  renderedEvidenceIds = new Set();
  $("#evidence-records").replaceChildren();
  appendEvidenceRecords(evidenceRecords.slice(0, evidenceVisible));
}

function progressiveEvidenceLinks(root, ids) {
  let visible = 0;
  const render = (all = false) => {
    visible = all ? ids.length : Math.min(visible + 100, ids.length);
    root.replaceChildren(...ids.slice(0, visible).map(evidenceLink));
    if (visible < ids.length) {
      const more = document.createElement("button");
      more.type = "button";
      more.className = "category-more";
      more.textContent = `Load 100 more of ${ids.length}`;
      more.addEventListener("click", () => render());
      root.append(more);
    }
  };
  return { first: () => visible || render(), all: () => render(true) };
}

function renderReport(payload) {
  const report = payload.report;
  const assessment = payload.assessment;
  const disposition = assessment?.disposition || "unassessed";
  document.documentElement.dataset.disposition = disposition;
  document.title = assessment
    ? `${disposition.toUpperCase()} — ${assessment.verdict}`
    : "UNASSESSED — RateMyPCB manufacturing release review";
  $("#disposition").textContent = disposition;
  $("#verdict").textContent =
    assessment?.verdict || "Engineering assessment required";
  $("#scope").textContent = report.reviewScope;
  const artifact = report.input.selectedBoard || report.input.kind;
  const revision = report.evidence[0]?.provenance?.artifactDigest;
  $("#source").textContent = revision
    ? `${artifact} · digest ${revision.slice(0, 16)}…`
    : artifact;
  $("#source").title = revision ? `Full artifact digest: ${revision}` : "";
  $("#evidence-time").textContent = evidenceFreshness(report);
  $("#assessment-note").textContent =
    assessment?.rationale || "No engineering assessment was supplied.";
  renderRefs($("#verdict-refs"), assessment?.verdictEvidenceRefs || []);

  const actions = assessment?.actions || [];
  $("#actions-card").hidden = !actions.length;
  $("#actions").replaceChildren(
    ...actions.map((item) =>
      entry(item.title, item.rationale, `P${item.priority}`, item.evidenceRefs),
    ),
  );
  const questions = assessment?.questions || [];
  $("#questions-card").hidden = !questions.length;
  $("#questions").replaceChildren(
    ...questions.map((item) =>
      entry(item.question, null, null, item.evidenceRefs),
    ),
  );

  $("#assessment-score").textContent = assessment
    ? `${assessment.rating}/10`
    : "Not assessed";
  $("#evidence-score").textContent = `${report.score.value}/10`;
  $("#approval").textContent = String(report.approvalEligible);
  renderRequiredEvidence(report);
  renderBom(report);

  const assessmentCategories = assessment?.categorySummaries || [];
  $("#assessment-categories-card").hidden = !assessmentCategories.length;
  $("#assessment-categories").replaceChildren(
    ...assessmentCategories.map((item) =>
      entry(item.categoryId, item.summary, null, item.evidenceRefs),
    ),
  );

  categoryRenderers = [];
  $("#categories").replaceChildren(
    ...report.categories.map((category) => {
      const node = document.createElement("details");
      node.className = "category";
      const heading = document.createElement("summary");
      heading.append(
        Object.assign(document.createElement("span"), {
          className: "category-name",
          textContent: category.label,
        }),
        Object.assign(document.createElement("span"), {
          className: `tag ${normalizedStatus(category.status)}`,
          textContent: normalizedStatus(category.status).replaceAll("-", " "),
        }),
      );
      const columns = document.createElement("div");
      columns.className = "category-columns";
      const checks = document.createElement("section");
      checks.append(Object.assign(document.createElement("h4"), { textContent: "Coverage claims" }));
      const checkLinks = document.createElement("div");
      checks.append(checkLinks);
      const findings = document.createElement("section");
      findings.append(Object.assign(document.createElement("h4"), { textContent: "Finding claims" }));
      const findingLinks = document.createElement("div");
      findings.append(findingLinks);
      const renderers = [
        progressiveEvidenceLinks(checkLinks, category.coverageIds),
        progressiveEvidenceLinks(findingLinks, category.findingIds),
      ];
      categoryRenderers.push(...renderers);
      node.addEventListener("toggle", () => {
        if (node.open) renderers.forEach((renderer) => renderer.first());
      });
      columns.append(checks, findings);
      node.append(heading, columns);
      return node;
    }),
  );
  renderSchematicEvidence(report);
  renderEvidenceRecords(report);

  $("#limitations").replaceChildren(
    ...report.limitations.map((text, index) => {
      const evidenceRefs = report.limitationEvidenceRefs?.[index] || [];
      const item = document.createElement("li");
      item.append(document.createTextNode(text));
      if (evidenceRefs.length) {
        const refs = document.createElement("div");
        refs.className = "evidence-refs";
        renderRefs(refs, evidenceRefs);
        item.append(refs);
      } else {
        const legacy = document.createElement("small");
        legacy.className = "legacy-evidence-note";
        legacy.textContent = "Legacy report: evidence reference not provided.";
        item.append(legacy);
      }
      return item;
    }),
  );
  $("#disclaimer").textContent = report.disclaimer;
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
    checkbox.addEventListener("change", () =>
      viewer.setLayerVisible(layer.id, checkbox.checked),
    );
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
  if (!model)
    root.textContent = "No viewable layers were included in this review.";
}

function show(kind) {
  const model = kind === "gerber" ? gerberModel : boardModel;
  viewer.setModel(model);
  renderLayers(model);
  for (const [button, active] of [
    [$("#board-tab"), kind === "board"],
    [$("#gerber-tab"), kind === "gerber"],
  ]) {
    button.classList.toggle("active", active);
    button.setAttribute("aria-pressed", String(active));
  }
  $("#viewer-note").textContent =
    model?.warnings?.join(" ") ||
    (model
      ? `${model.primitives} supported primitives rendered locally.`
      : "No local geometry is available for this view.");
  const layers = (model?.layers || []).map((layer) =>
    `${layer.name || layer.id}: ${(layer.strokes?.length || 0) + (layer.flashes?.length || 0) + (layer.polygons?.length || 0)} drawable items`,
  );
  $("#viewer-fallback").textContent = [
    model ? `${kind} source; ${model.primitives} supported primitives.` : "No local geometry is available.",
    ...layers,
    ...(model?.warnings || []),
  ].join(" ");
}

async function load() {
  let payload = globalThis.RATEMYPCB_PAYLOAD;
  if (!payload) {
    if (!token)
      throw new Error("The local viewer capability token is missing.");
    const response = await fetch("/session", {
      headers: { "x-ratemypcb-token": token },
      cache: "no-store",
    });
    if (!response.ok)
      throw new Error(
        `The local viewer session is unavailable (${response.status}).`,
      );
    payload = await response.json();
  }
  renderReport(payload);
  if (!token && initialHash === "bom-panel") reportTab("bom");
  if (!token && initialHash.startsWith("evidence-")) {
    reportTab("overview");
    const publicId = initialHash.slice("evidence-".length);
    requestAnimationFrame(() => ensureEvidenceRecord(publicId)?.focus());
  }
  if (payload.board?.source) boardModel = parseKiCadView(payload.board.source);
  const gerbers = [];
  let budget = 750000;
  const failures = [...(payload.failures || [])];
  for (const file of payload.gerbers || []) {
    try {
      const model = parseGerber(file.source, file.path, {
        maxPrimitives: budget,
      });
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
$("#overview-tab").addEventListener("click", () => reportTab("overview"));
$("#bom-tab").addEventListener("click", () => reportTab("bom"));
const reportTabs = [$("#overview-tab"), $("#bom-tab")];
for (const [index, tab] of reportTabs.entries()) {
  tab.addEventListener("keydown", (event) => {
    let nextIndex;
    if (["ArrowLeft", "ArrowRight"].includes(event.key)) nextIndex = index ? 0 : 1;
    else if (event.key === "Home") nextIndex = 0;
    else if (event.key === "End") nextIndex = reportTabs.length - 1;
    else return;
    event.preventDefault();
    reportTabs[nextIndex].click();
    reportTabs[nextIndex].focus();
  });
}
for (const control of [$("#bom-search"), $("#bom-filter"), $("#bom-sort")]) {
  control.addEventListener("input", () => {
    bomVisible = 100;
    updateBomRows();
  });
}
$("#bom-controls").addEventListener("submit", (event) => event.preventDefault());
$("#bom-more").addEventListener("click", () => {
  bomVisible += 100;
  updateBomRows();
});
$("#evidence-more").addEventListener("click", () => {
  evidenceVisible += 100;
  appendEvidenceRecords(evidenceRecords.slice(0, evidenceVisible));
});
addEventListener("beforeprint", () => {
  document.querySelectorAll("details").forEach((details) => {
    details.open = true;
  });
  bomVisible = bomLines.length;
  updateBomRows();
  evidenceVisible = evidenceRecords.length;
  appendEvidenceRecords(evidenceRecords);
  categoryRenderers.forEach((renderer) => renderer.all());
});
$("#fit").addEventListener("click", () => viewer.fit());
$("#zoom-in").addEventListener("click", () => viewer.zoom(1.2));
$("#zoom-out").addEventListener("click", () => viewer.zoom(1 / 1.2));
load().catch((error) => {
  $("#disposition").textContent = "unavailable";
  $("#source").textContent = error.message;
  $("#source").classList.add("error");
});
