import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";
import test from "node:test";
import {
  filterAndSortBomLines,
  schematicEvidenceRefs,
  supplyDetailGroups,
} from "../crates/ratemypcb-cli/assets/report-view-model.mjs";

const root = new URL("./fixtures/report-ux/", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8");
const fixture = (path) => JSON.parse(read(path));
const html = readFileSync(
  new URL("../crates/ratemypcb-cli/assets/local-viewer.html", import.meta.url),
  "utf8",
);
const css = readFileSync(
  new URL("../crates/ratemypcb-cli/assets/local-viewer.css", import.meta.url),
  "utf8",
);
const js = readFileSync(
  new URL("../crates/ratemypcb-cli/assets/local-viewer.js", import.meta.url),
  "utf8",
);
const ci = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");
const manifest = fixture("corpus-manifest.json");
const blockedReport = fixture("blocked-small.report.json");
const blockedAssessment = fixture("blocked-small.assessment.json");
const approvedReport = fixture("approved-small.report.json");
const approvedAssessment = fixture("approved-small.assessment.json");

function model(lines, filter = "all", query = "") {
  return filterAndSortBomLines(lines, query, filter, "release-impact");
}

function claims(report, assessment) {
  return [
    assessment.verdictEvidenceRefs,
    ...assessment.categorySummaries.map((item) => item.evidenceRefs),
    ...assessment.actions.map((item) => item.evidenceRefs),
    ...assessment.questions.map((item) => item.evidenceRefs),
    ...report.requiredEvidence.map((item) => [item.evidenceId]),
    ...report.limitations.map((_, index) => report.limitationEvidenceRefs[index]),
    ...report.findings.map((item) => [item.id]),
    ...report.coverage.map((item) => [item.id]),
  ];
}

test("redistribution-safe corpus hashes and metadata match", () => {
  assert.equal(manifest.license, "CC0-1.0");
  assert.match(manifest.source, /synthetic/i);
  for (const item of manifest.fixtures) {
    const bytes = readFileSync(new URL(item.path, root));
    assert.equal(createHash("sha256").update(bytes).digest("hex"), item.sha256);
    assert.doesNotMatch(bytes.toString(), /(?:https?:\/\/|[A-Z]:\\|\/Users\/|@example\.)/);
    const parsed = JSON.parse(bytes);
    assert.equal(parsed.disposition ?? null, item.expectedDisposition);
    assert.equal(parsed.evidence?.length ?? 0, item.expectedEvidenceIds);
    assert.equal(parsed.actions?.[0]?.title ?? null, item.expectedFirstAction);
  }
});

test("goldens preserve decision hierarchy and bounded actions", () => {
  for (const [report, assessment] of [
    [blockedReport, blockedAssessment],
    [approvedReport, approvedAssessment],
  ]) {
    assert.ok(assessment.verdict.length <= 60);
    assert.ok(assessment.actions.length <= 3);
    assert.ok(report.reviewScope && report.input.selectedBoard && report.freshness);
  }
  const decision = html.indexOf('data-report-landmark="release-decision"');
  const completeness = html.indexOf('data-report-landmark="completeness"');
  const scores = html.indexOf('data-report-landmark="scores"');
  assert.ok(decision < completeness && completeness < scores);
});

test("every visible claim resolves to complete provenance", () => {
  for (const [report, assessment] of [
    [blockedReport, blockedAssessment],
    [approvedReport, approvedAssessment],
  ]) {
    const records = new Map(report.evidence.map((record) => [record.id, record]));
    assert.equal(records.size, report.evidence.length);
    for (const refs of claims(report, assessment)) {
      assert.ok(refs.length > 0);
      for (const id of refs) {
        const provenance = records.get(id)?.provenance;
        assert.ok(provenance, `missing ${id}`);
        assert.ok(provenance.artifactDigest && provenance.producer.name);
        assert.ok(provenance.producer.version && provenance.location);
        assert.ok(provenance.evidenceClass && provenance.confidence);
        assert.ok(provenance.freshness && provenance.observedAt);
      }
    }
  }
});

test("unknown states stay named and cannot approve", () => {
  assert.equal(blockedReport.approvalEligible, false);
  assert.equal(blockedAssessment.disposition, "blocked");
  assert.deepEqual(
    blockedReport.requiredEvidence.map((item) => item.execution),
    ["completed", "completed", "not_run", "not_provided"],
  );
  assert.ok(blockedReport.bom.lines.some((line) => line.releaseImpact.status === "not-checked"));
  assert.doesNotMatch(
    `${blockedAssessment.verdict} ${blockedAssessment.rationale}`,
    /\b(?:approve|passed|ready)\b/i,
  );
});

test("BOM release-impact ordering and filters preserve every row", () => {
  const lines = blockedReport.bom.lines;
  const sorted = model(lines);
  assert.deepEqual(sorted.slice(0, 3).map(({ line }) => line.releaseImpact.status), [
    "attention",
    "attention",
    "attention",
  ]);
  for (const status of ["attention", "not-checked", "pass"]) {
    const expected = lines.filter((line) => line.releaseImpact.status === status);
    assert.deepEqual(model(lines, status).map(({ line }) => line.lineNumber), expected.map((line) => line.lineNumber));
  }
  assert.equal(model(lines).length, lines.length);
});

test("10 to 10,000 line models remain deterministic and bounded", () => {
  for (const count of [10, 100, 1000, 10000]) {
    const lines = Array.from({ length: count }, (_, index) => ({
      ...structuredClone(blockedReport.bom.lines[index % 10]),
      lineNumber: index + 2,
      references: [`R${index + 1}`],
    }));
    const start = performance.now();
    const output = model(lines);
    const elapsed = performance.now() - start;
    assert.equal(output.length, count);
    assert.equal(new Set(output.map(({ line }) => line.lineNumber)).size, count);
    const findings = Array.from({ length: count }, (_, index) => ({
      id: `ev-${index.toString(16).padStart(64, "0")}`,
      evidence: `Synthetic finding ${index}`,
    }));
    assert.equal(new Set(findings.map(({ id }) => id)).size, count);
    assert.equal(findings.at(-1).evidence, `Synthetic finding ${count - 1}`);
    assert.ok(elapsed < 2000, `${count} lines took ${elapsed.toFixed(1)}ms`);
  }
  assert.match(js, /let bomVisible = 100/);
  assert.match(js, /bomVisible \+= 100/);
  assert.match(js, /let evidenceVisible = 100/);
  assert.match(js, /appendEvidenceRecords/);
});

test("static accessibility, keyboard, fallback, responsive, and print contracts exist", () => {
  const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map((match) => match[1]);
  assert.equal(new Set(ids).size, ids.length);
  for (const [, target] of html.matchAll(/aria-(?:controls|labelledby|describedby)="([^"]+)"/g)) {
    assert.ok(ids.includes(target), `missing ARIA target ${target}`);
  }
  assert.match(html, /<caption>Bill of materials release-impact risk matrix<\/caption>/);
  assert.match(html, /<canvas[\s\S]*aria-describedby="viewer-fallback"[\s\S]*>[^<]+<\/canvas>/);
  assert.match(html, /aria-label="Zoom out"/);
  assert.match(html, /aria-label="Zoom in"/);
  assert.match(html, /id="board-tab"[\s\S]*aria-pressed="false"/);
  assert.match(js, /event\.key === "Home"/);
  assert.match(js, /event\.key === "End"/);
  assert.match(js, /navigator\.clipboard\?\.writeText/);
  assert.match(js, /target\.focus\(\)/);
  assert.match(js, /details\.open = true/);
  assert.match(js, /provenance\.observedAt \?\? "Not provided"/);
  assert.match(js, /Legacy report: evidence reference not provided/);
  assert.doesNotMatch(js, /decodeURIComponent\(location\.hash/);
  assert.match(css, /:focus-visible/);
  assert.match(css, /@media \(forced-colors: active\)/);
  assert.match(css, /@media print/);
  assert.match(css, /\.report-panel\[hidden\][\s\S]*display: block !important/);
  assert.match(css, /details\.limitations > :not\(summary\)/);
  assert.match(css, /--amber: #75470b/);
  assert.match(css, /@media \(max-width: 760px\)/);
});

test("viewer preserves all decision-relevant supply details", () => {
  const groups = supplyDetailGroups({
    providerChecks: [{provider: "lcsc", status: "not-checked", errorKind: null, retrievedAtUnix: null, upstreamAtUnix: null, provenance: null}],
    offers: [{provider: "mouser", seller: "Seller", sellerOriginal: "Original", sku: "SKU", authorization: "authorized", stockStatus: "in-stock", packaging: "reel", region: "US", stock: 9, moq: 2, orderMultiple: 2, purchasableQuantity: 4, factoryLeadTimeDays: 7, applicableUnitPrice: "1.2500", currency: "USD", retrievedAtUnix: 10, upstreamAtUnix: 9, legalExpiresAtUnix: 20, usable: true, provenance: "offers/0"}],
    lifecycleConflict: true,
    lifecycleAssertions: [{provider: "mouser", raw: "Active", normalized: "active", observedAtUnix: 9, provenance: "lifecycle/0"}],
    alternateCandidates: [{manufacturer: "Acme", mpn: "ALT", source: "mouser", evidenceId: "candidate-1", provenance: "alternates/0"}],
    approvedAlternates: [{manufacturer: "Acme", mpn: "ALT", authorityKind: "engineering", authority: "Release", approvedAtUnix: 10, evidenceRefs: ["candidate-1"]}],
  });
  const rendered = groups.flatMap(([, lines]) => lines).join("\n");
  for (const expected of ["lcsc: not-checked", "Original", "SKU", "in-stock", "1.2500 USD", "lead time days 7", "legal expiry 20", "usable true", "Lifecycle conflict: yes", "candidate-1", "engineering: Release"]) {
    assert.match(rendered, new RegExp(expected));
  }
  assert.match(js, /supplyDetailGroups\(line\)/);
  assert.match(js, /line\.unitPriceDecimal \?\? line\.unitPrice/);
});

test("schematic viewer consumes core provenance and gate values safely", () => {
  const report = {
    evidence: [
      { checkId: "schematic-evidence", id: "generic-ref" },
      { checkId: "schematic-erc", id: "erc-ref" },
      { checkId: "schematic-parity", id: "parity-ref" },
    ],
  };
  assert.deepEqual(schematicEvidenceRefs(report), ["generic-ref"]);
  assert.deepEqual(schematicEvidenceRefs(report, "schematic-erc"), ["erc-ref"]);
  assert.deepEqual(schematicEvidenceRefs(report, "schematic-parity"), ["parity-ref"]);
  assert.deepEqual(
    schematicEvidenceRefs(
      { evidence: report.evidence.filter((record) => record.checkId !== "schematic-erc") },
      "schematic-erc",
    ),
    ["generic-ref"],
  );
  assert.deepEqual(schematicEvidenceRefs({ evidence: [] }, "schematic-erc"), []);

  const renderer = js.match(
    /function renderSchematicEvidence\(report\) \{[\s\S]*?\n\}\n\nfunction renderEvidenceRecords/,
  )?.[0];
  assert.ok(renderer);
  assert.match(renderer, /schematic\.sourcePair/);
  assert.match(renderer, /mismatch\.location/);
  assert.match(renderer, /mismatch\.gateImpact/);
  assert.match(renderer, /marker\.structuralLocation/);
  assert.match(renderer, /marker\.excluded \?\? "unknown"/);
  assert.match(renderer, /const nativeRefs = schematicEvidenceRefs\(report, checkId\)/);
  assert.equal(renderer.match(/\bnativeRefs,/g)?.length, 2, "report and marker entries must share channel refs");
  assert.match(js, /copy\.textContent = evidence/);
  assert.doesNotMatch(js, /innerHTML\s*=/);
  assert.doesNotMatch(js, /schematic[^\n]*(?:approvalEligible|requiredEvidence)\s*=/);
});

test("viewer uses no external runtime resource", () => {
  assert.doesNotMatch(html, /<(?:script|link|img)[^>]+(?:src|href)="https?:\/\//i);
  assert.doesNotMatch(js, /fetch\("https?:\/\//i);
});

test("CI runs the active Phase 2 suites and schema", () => {
  assert.match(ci, /tests\/board-view\.test\.mjs/);
  assert.match(ci, /tests\/report-contract\.test\.mjs/);
  assert.match(ci, /tests\/report-ux\.test\.mjs/);
  assert.match(ci, /diff -u schemas\/report-2\.0\.json/);
  assert.doesNotMatch(ci, /diff -u schemas\/report-1\.0\.json/);
});
