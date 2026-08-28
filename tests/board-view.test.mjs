import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const viewerSource = readFileSync(new URL("../crates/ratemypcb-cli/assets/board-view.js", import.meta.url), "utf8");
const { parseGerber } = await import(`data:text/javascript;base64,${Buffer.from(viewerSource).toString("base64")}`);
const layers = [
  ["board.gbl", 77_000],
  ["board.gbo", 77_000],
  ["board.gbs", 77_000],
  ["board.gko", 77_000],
  ["board.gtl", 77_000],
  ["board.gto", 77_000],
  ["board.gtp", 77_000],
  ["board.gts", 77_000],
  ["drill-drawing.gbr", 77_957],
];

function syntheticGerber(count) {
  return `%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.1*%\nD10*\n${"X0Y0D03*\n".repeat(count)}M02*\n`;
}

test("renders a synthetic large Gerber package within viewer limits", () => {
  let budget = 750_000;
  for (const [filename, count] of layers) {
    const model = parseGerber(syntheticGerber(count), filename, { maxPrimitives: budget });
    assert(model.primitives > 0, `${filename} should contain drawable geometry`);
    budget -= model.primitives;
  }
  assert.equal(750_000 - budget, 693_957);
});
