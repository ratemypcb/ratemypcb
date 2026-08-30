import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (path) =>
  readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
const parse = (path) => JSON.parse(read(path));
const clone = (value) => structuredClone(value);

const report = parse("tests/fixtures/decision-report/blocked-report.json");
const assessment = parse(
  "tests/fixtures/decision-report/blocked-assessment.json",
);
const viewerHtml = read("crates/ratemypcb-cli/assets/local-viewer.html");
const viewerJs = read("crates/ratemypcb-cli/assets/local-viewer.js");

const incompleteExecutions = new Set([
  "not_run",
  "not_provided",
  "failed",
  "unsupported",
  "unknown",
]);
const passLike =
  /\b(?:approve|approved|clear|cleared|pass|passed|ready|would manufacture)\b/i;

function contractError(condition, message) {
  if (!condition) throw new Error(message);
}

function evaluate(candidateReport, candidateAssessment, html = viewerHtml) {
  contractError(candidateReport.schemaVersion === "2.0", "report version");
  contractError(
    candidateAssessment.assessmentSchemaVersion === "2.0",
    "assessment version",
  );
  contractError(
    typeof candidateAssessment.disposition === "string" &&
      ["approve", "revise", "blocked"].includes(
        candidateAssessment.disposition,
      ),
    "one unambiguous disposition",
  );
  contractError(
    typeof candidateAssessment.verdict === "string" &&
      candidateAssessment.verdict.length > 0 &&
      [...candidateAssessment.verdict].length <= 60,
    "bounded verdict",
  );
  contractError(
    Array.isArray(candidateAssessment.actions) &&
      candidateAssessment.actions.length <= 3,
    "at most three actions",
  );
  contractError(
    Array.isArray(candidateReport.requiredEvidence) &&
      candidateReport.requiredEvidence.length > 0,
    "required evidence completeness",
  );
  contractError(
    typeof candidateReport.freshness === "string",
    "overall freshness",
  );
  contractError(
    !candidateReport.requiredEvidence.some(({ checkId }) =>
      checkId.startsWith("schematic"),
    ),
    "schematic evidence is not required before promotion",
  );
  for (const mismatch of candidateReport.schematic?.mismatches || []) {
    contractError(
      mismatch.gateImpact === "evidence_only",
      "schematic mismatch remains evidence only",
    );
  }
  for (const native of [
    candidateReport.schematic?.nativeErc,
    candidateReport.schematic?.nativeParity,
  ]) {
    for (const marker of native?.violations || []) {
      contractError(
        marker.excluded !== true || marker.structuralLocation,
        "excluded marker retains provenance location",
      );
    }
  }

  const records = candidateReport.evidence;
  contractError(Array.isArray(records), "evidence records");
  const ids = records.map(({ id }) => id);
  contractError(new Set(ids).size === ids.length, "unique public evidence IDs");
  const occurrences = [
    ...candidateReport.findings.map(({ id }) => id),
    ...candidateReport.coverage.map(({ id }) => id),
  ];
  contractError(
    occurrences.length === ids.length &&
      occurrences.every((id) => ids.includes(id)),
    "one public ID per evidence occurrence",
  );

  const missing = candidateReport.requiredEvidence.filter((item) => {
    contractError(
      typeof item.freshness === "string",
      "required evidence freshness",
    );
    contractError(ids.includes(item.evidenceId), "required evidence reference");
    return (
      incompleteExecutions.has(item.execution) ||
      item.result !== "pass" ||
      !["current", "not_applicable"].includes(item.freshness)
    );
  });
  contractError(missing.length > 0, "explicit missing required evidence");
  if (missing.length) {
    const decisionText = [
      candidateAssessment.disposition,
      candidateAssessment.verdict,
      candidateAssessment.rationale,
      ...candidateAssessment.categorySummaries.map(({ summary }) => summary),
      ...candidateAssessment.actions.flatMap(({ title, rationale }) => [
        title,
        rationale,
      ]),
    ].join(" ");
    contractError(
      !candidateReport.approvalEligible &&
        candidateAssessment.disposition !== "approve" &&
        !passLike.test(decisionText),
      "incomplete evidence cannot carry approval or pass-like labels",
    );
  }

  const claims = [
    ["verdict", candidateAssessment.verdictEvidenceRefs],
    ...candidateReport.limitationEvidenceRefs.map((refs) => ["limitation", refs]),
    ...candidateAssessment.categorySummaries.map(({ evidenceRefs }) => [
      "category",
      evidenceRefs,
    ]),
    ...candidateAssessment.actions.map(({ evidenceRefs }) => [
      "action",
      evidenceRefs,
    ]),
    ...candidateAssessment.questions.map(({ evidenceRefs }) => [
      "question",
      evidenceRefs,
    ]),
  ];
  for (const [label, refs] of claims) {
    contractError(
      Array.isArray(refs) && refs.length > 0,
      `${label} evidence references`,
    );
    contractError(
      new Set(refs).size === refs.length,
      `${label} unique evidence references`,
    );
    contractError(
      refs.every((id) => ids.includes(id)),
      `${label} evidence reference integrity`,
    );
  }

  const decision = html.indexOf('data-report-landmark="release-decision"');
  const completeness = html.indexOf('data-report-landmark="completeness"');
  const scores = html.indexOf('data-report-landmark="scores"');
  contractError(
    decision >= 0 && decision < completeness && completeness < scores,
    "decision and completeness before scores",
  );
  contractError(
    html.match(/data-report-landmark="release-decision"/g)?.length === 1,
    "one decision landmark",
  );
  contractError(
    viewerJs.includes("function evidenceAnchor(publicId)"),
    "shared anchor helper",
  );
  contractError(
    viewerJs.includes("link.href = `#${evidenceAnchor(publicId)}`") &&
      viewerJs.includes("node.id = evidenceAnchor(record.id)"),
    "shared anchor helper wiring",
  );
}

test("sanitized blocked report and assessment satisfy the contract", () => {
  assert.doesNotThrow(() => evaluate(report, assessment));
});

test("missing or competing disposition is rejected", async (t) => {
  await t.test("missing", () => {
    const mutation = clone(assessment);
    delete mutation.disposition;
    assert.throws(() => evaluate(report, mutation), /unambiguous disposition/);
  });
  await t.test("competing", () => {
    const mutation = clone(assessment);
    mutation.disposition = ["blocked", "approve"];
    assert.throws(() => evaluate(report, mutation), /unambiguous disposition/);
  });
});

test("required evidence cannot fail open with approval or pass-like labels", () => {
  const mutatedReport = clone(report);
  mutatedReport.requiredEvidence[0].execution = "failed";
  mutatedReport.requiredEvidence[0].result = "fail";
  mutatedReport.approvalEligible = true;
  const mutatedAssessment = clone(assessment);
  mutatedAssessment.disposition = "approve";
  mutatedAssessment.verdict = "Passed and ready";
  assert.throws(
    () => evaluate(mutatedReport, mutatedAssessment),
    /cannot carry approval or pass-like labels/,
  );
});

test("broken assessment evidence reference is rejected", () => {
  const mutation = clone(assessment);
  mutation.questions[0].evidenceRefs[0] = `ev-${"f".repeat(64)}`;
  assert.throws(
    () => evaluate(report, mutation),
    /question evidence reference integrity/,
  );
});

test("duplicate public evidence ID is rejected", () => {
  const mutation = clone(report);
  mutation.evidence.push(clone(mutation.evidence[0]));
  assert.throws(
    () => evaluate(mutation, assessment),
    /unique public evidence IDs/,
  );
});

test("decision completeness and freshness are required", async (t) => {
  await t.test("completeness", () => {
    const mutation = clone(report);
    delete mutation.requiredEvidence;
    assert.throws(
      () => evaluate(mutation, assessment),
      /required evidence completeness/,
    );
  });
  await t.test("freshness", () => {
    const mutation = clone(report);
    delete mutation.freshness;
    assert.throws(() => evaluate(mutation, assessment), /overall freshness/);
  });
});

test("four assessment actions exceed the core-aligned budget", () => {
  const mutation = clone(assessment);
  mutation.actions = Array.from({ length: 4 }, (_, index) => ({
    ...clone(assessment.actions[0]),
    priority: index + 1,
    title: `Action ${index + 1}`,
  }));
  assert.throws(() => evaluate(report, mutation), /at most three actions/);
});

test("score-first landmark ordering is rejected", () => {
  const mutation = viewerHtml
    .replace(
      'data-report-landmark="release-decision"',
      'data-report-landmark="temporary"',
    )
    .replace(
      'data-report-landmark="scores"',
      'data-report-landmark="release-decision"',
    )
    .replace(
      'data-report-landmark="temporary"',
      'data-report-landmark="scores"',
    );
  assert.throws(() => evaluate(report, assessment, mutation), /before scores/);
});

test("schematic evidence cannot be silently promoted or made blocking", async (t) => {
  await t.test("required evidence mutation", () => {
    const mutation = clone(report);
    mutation.requiredEvidence.push({
      ...clone(mutation.requiredEvidence[0]),
      checkId: "schematic-erc",
    });
    assert.throws(
      () => evaluate(mutation, assessment),
      /not required before promotion/,
    );
  });
  await t.test("blocking mismatch mutation", () => {
    const mutation = clone(report);
    mutation.schematic = {
      ...mutation.schematic,
      mismatches: [{ gateImpact: "blocking" }],
    };
    assert.throws(
      () => evaluate(mutation, assessment),
      /remains evidence only/,
    );
  });
});

test("schema, viewer, and docs expose one schematic capability contract", () => {
  const schema = parse("schemas/report-2.0.json");
  const skill = read("skills/review-pcb-dfm/SKILL.md");
  const reference = read("skills/review-pcb-dfm/references/report-contract.md");
  const readme = read("README.md");
  const sourcePair = schema.$defs.schematicSourcePair;
  assert.deepEqual(sourcePair.required, [
    "projectIdentity",
    "schematicPath",
    "schematicDigest",
    "boardPath",
    "boardDigest",
  ]);
  assert.equal(schema.$defs.schematicMismatch.properties.gateImpact.const, "evidence_only");
  assert.match(viewerJs, /function renderSchematicEvidence\(report\)/);
  assert.match(viewerJs, /marker\.excluded \?\? "unknown"/);
  assert.match(viewerJs, /mismatch\.gateImpact/);
  for (const text of [skill, reference, readme]) {
    assert.match(text, /--schematic/);
    assert.match(text, /evidence_only/);
    assert.match(text, /8, 9,\s+(?:and|or) 10/);
    assert.match(text, /Altium/);
    assert.match(text, /generic netlist|generic-netlist/i);
    assert.match(text, /ZIP/);
  }
});

test("schema, viewer, docs, and CI expose one manufacturing authority contract", () => {
  const schema = parse("schemas/report-2.0.json");
  const skill = read("skills/review-pcb-dfm/SKILL.md");
  const reference = read("skills/review-pcb-dfm/references/report-contract.md");
  const readme = read("README.md");
  const ci = read(".github/workflows/ci.yml");
  const coreCargo = read("crates/ratemypcb-core/Cargo.toml");
  for (const field of [
    "sourcePair", "nativeReconciliationSource", "reconciliations", "x2Attributes",
  ]) {
    assert.ok(schema.$defs.fabricationReview.required.includes(field));
  }
  assert.deepEqual(schema.$defs.manufacturingReconciliation.properties.family.enum, [
    "product", "layers", "profile", "drills", "extents", "connectivity",
  ]);
  assert.deepEqual(schema.$defs.manufacturingReconciliation.properties.status.enum, [
    "match", "mismatch", "not_checked",
  ]);
  assert.match(viewerJs, /function renderFabricationEvidence\(report\)/);
  assert.match(viewerJs, /fabrication\.sourcePair/);
  assert.match(viewerJs, /item\.smallestEvidenceAction/);
  const renderer = viewerJs.match(
    /function renderFabricationEvidence\(report\) \{[\s\S]*?\n\}\n\nfunction renderEvidenceRecords/,
  )?.[0];
  assert.ok(renderer);
  assert.doesNotMatch(renderer, /parseGerber|inspectGerberSet/);
  assert.doesNotMatch(renderer, /approvalEligible\s*=|requiredEvidence\s*=/);
  for (const text of [skill, reference, readme]) {
    assert.match(text, /Gerber\/X2/);
    assert.match(text, /strict XNC/i);
    assert.match(text, /KiCad\/LibrePCB|KiCad.*LibrePCB/s);
    assert.match(text, /local-only/);
    assert.match(text, /ODB\+\+/);
    assert.match(text, /IPC-2581/);
    assert.match(text, /presentation-only/);
  }
  assert.match(ci, /cargo test --all --locked/);
  assert.match(
    coreCargo,
    /gerber_parser = \{ git = "https:\/\/github\.com\/ratemypcb\/gerber-parser\.git", rev = "54004bc52c11699b49cd287a49135380feee86b3" \}/,
  );
  assert.match(ci, /node --check crates\/ratemypcb-cli\/assets\/local-viewer\.js/);
});

test("active report and assessment declarations are exactly 2.0", () => {
  const core = read("crates/ratemypcb-core/src/lib.rs");
  const viewer = read("crates/ratemypcb-cli/src/viewer.rs");
  const skill = read("skills/review-pcb-dfm/SKILL.md");
  const reference = read("skills/review-pcb-dfm/references/report-contract.md");
  const reportSchema = parse("schemas/report-2.0.json");
  const assessmentSchema = parse("schemas/assessment-2.0.json");

  assert.match(core, /^pub const SCHEMA_VERSION: &str = "2\.0";$/m);
  assert.match(core, /^pub const ASSESSMENT_SCHEMA_VERSION: &str = "2\.0";$/m);
  assert.match(
    core,
    /^ {6}"\$id": "https:\/\/ratemypcb\.com\/schemas\/report-2\.0\.json",$/m,
  );
  assert.match(
    core,
    /^ {6}"\$id": "https:\/\/ratemypcb\.com\/schemas\/assessment-2\.0\.json",$/m,
  );
  assert.equal(
    reportSchema.$id,
    "https://ratemypcb.com/schemas/report-2.0.json",
  );
  assert.equal(reportSchema.properties.schemaVersion.const, "2.0");
  assert.equal(
    assessmentSchema.$id,
    "https://ratemypcb.com/schemas/assessment-2.0.json",
  );
  assert.equal(
    assessmentSchema.properties.assessmentSchemaVersion.const,
    "2.0",
  );
  assert.match(viewer, /^ {12}"schemaVersion": "2\.0",$/m);
  assert.match(
    skill,
    /Require report schema 2\.0 and assessment schema 2\.0\./,
  );
  assert.match(
    reference,
    /^Active schemas: report 2\.0 and assessment 2\.0\.$/m,
  );
});
