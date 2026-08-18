const COLORS = {
  edge: "#dafe45",
  front: "#ff7145",
  back: "#5297ff",
  inner: "#b991ff",
  mask: "#34b98a",
  silk: "#f2efe5",
  paste: "#e6a23c",
  drill: "#d7cab8",
  document: "#82958b",
  other: "#b991ff",
};

const GERBER_ROLE_DEFINITIONS = {
  "copper-top": { family: "Copper", side: "Top", label: "Top copper", color: COLORS.front, order: 10 },
  "copper-inner": { family: "Copper", side: "Inner", label: "Inner copper", color: COLORS.inner, order: 20 },
  "copper-bottom": { family: "Copper", side: "Bottom", label: "Bottom copper", color: COLORS.back, order: 30 },
  "mask-top": { family: "Solder mask", side: "Top", label: "Top solder mask", color: COLORS.mask, order: 40 },
  "mask-bottom": { family: "Solder mask", side: "Bottom", label: "Bottom solder mask", color: COLORS.mask, order: 50 },
  "paste-top": { family: "Paste", side: "Top", label: "Top paste", color: COLORS.paste, order: 60 },
  "paste-bottom": { family: "Paste", side: "Bottom", label: "Bottom paste", color: COLORS.paste, order: 70 },
  "silk-top": { family: "Legend", side: "Top", label: "Top silkscreen", color: COLORS.silk, order: 80 },
  "silk-bottom": { family: "Legend", side: "Bottom", label: "Bottom silkscreen", color: COLORS.silk, order: 90 },
  outline: { family: "Profile", side: "Board", label: "Board profile", color: COLORS.edge, order: 100 },
  "drill-drawing": { family: "Drill drawing", side: "Board", label: "Drill drawing", color: COLORS.drill, order: 110 },
  documentation: { family: "Documentation", side: "Board", label: "Fabrication drawing", color: COLORS.document, order: 120 },
  unknown: { family: "Unclassified", side: "Unknown", label: "Unclassified layer", color: COLORS.other, order: 999 },
};

const KICAD_ROUNDRECT_MACRO = Object.freeze([
  "4,1,4,$2,$3,$4,$5,$6,$7,$8,$9,$2,$3,0",
  "1,1,$1+$1,$2,$3",
  "1,1,$1+$1,$4,$5",
  "1,1,$1+$1,$6,$7",
  "1,1,$1+$1,$8,$9",
  "20,1,$1+$1,$2,$3,$4,$5,0",
  "20,1,$1+$1,$4,$5,$6,$7,0",
  "20,1,$1+$1,$6,$7,$8,$9,0",
  "20,1,$1+$1,$8,$9,$2,$3,0",
]);

function gerberRole(key, detectedBy) {
  return { key, ...GERBER_ROLE_DEFINITIONS[key], detectedBy };
}
/**
 * Resolve manufacturing intent from an X2 FileFunction attribute first and
 * conventional KiCad/Protel filenames second. Classification is deliberately
 * kept separate from parsing so an unsupported file can still appear in the
 * inventory with a useful role and error.
 */
export function classifyGerberLayer(filename = "layer.gbr", source = "") {
  const name = filename.toLowerCase().replace(/\\/g, "/").split("/").pop() || "";
  const fileFunction = source.match(/%TF\.FileFunction,([^*%]+)\*%/i)?.[1] || "";
  const attributes = fileFunction.split(",").map((part) => part.trim().toLowerCase()).filter(Boolean);
  const fn = attributes[0] || "";
  const rawSide = attributes.find((part) => part === "top" || part === "bottom" || part === "bot") || "";
  const side = rawSide === "bot" ? "bottom" : rawSide;
  if (fn === "copper") {
    if (side === "top" || attributes.includes("l1")) return gerberRole("copper-top", "X2 FileFunction");
    if (side === "bottom") return gerberRole("copper-bottom", "X2 FileFunction");
    return gerberRole("copper-inner", "X2 FileFunction");
  }
  if (fn === "soldermask") {
    const resolvedSide = side || (/(?:\.(?:gbs|sts)$|(?:^|[-_.])b(?:ack)?[-_.]?(?:mask|soldermask)\.(?:gbr|ger)$)/.test(name)
      ? "bottom"
      : /(?:\.(?:gts|stc)$|(?:^|[-_.])f(?:ront)?[-_.]?(?:mask|soldermask)\.(?:gbr|ger)$)/.test(name) ? "top" : "");
    return resolvedSide ? gerberRole(resolvedSide === "bottom" ? "mask-bottom" : "mask-top", side ? "X2 FileFunction" : "X2 FileFunction + filename side") : gerberRole("unknown", "incomplete X2 FileFunction");
  }
  if (fn === "legend" || fn === "silkscreen") {
    const resolvedSide = side || (/(?:\.(?:gbo|pls)$|(?:^|[-_.])b(?:ack)?[-_.]?(?:silks?|silkscreen|legend)\.(?:gbr|ger)$)/.test(name)
      ? "bottom"
      : /(?:\.(?:gto|plc)$|(?:^|[-_.])f(?:ront)?[-_.]?(?:silks?|silkscreen|legend)\.(?:gbr|ger)$)/.test(name) ? "top" : "");
    return resolvedSide ? gerberRole(resolvedSide === "bottom" ? "silk-bottom" : "silk-top", side ? "X2 FileFunction" : "X2 FileFunction + filename side") : gerberRole("unknown", "incomplete X2 FileFunction");
  }
  if (fn === "paste") {
    const resolvedSide = side || (/(?:\.(?:gbp|crs)$|(?:^|[-_.])b(?:ack)?[-_.]?paste\.(?:gbr|ger)$)/.test(name)
      ? "bottom"
      : /(?:\.(?:gtp|crc)$|(?:^|[-_.])f(?:ront)?[-_.]?paste\.(?:gbr|ger)$)/.test(name) ? "top" : "");
    return resolvedSide ? gerberRole(resolvedSide === "bottom" ? "paste-bottom" : "paste-top", side ? "X2 FileFunction" : "X2 FileFunction + filename side") : gerberRole("unknown", "incomplete X2 FileFunction");
  }
  if (fn === "profile") return gerberRole("outline", "X2 FileFunction");
  if (fn === "drillmap" || fn === "drilldrawing") return gerberRole("drill-drawing", "X2 FileFunction");
  if (["drawing", "fabricationdrawing", "assemblydrawing", "other"].includes(fn)) return gerberRole("documentation", "X2 FileFunction");
  if (fileFunction) return gerberRole("unknown", "unsupported X2 FileFunction");

  const detectedBy = "filename";
  if (/\.(gtl|cmp)$/.test(name) || /(?:^|[-_.])f(?:ront)?[-_.]?cu\.(?:gbr|ger)$/.test(name)) return gerberRole("copper-top", detectedBy);
  if (/\.(gbl|sol)$/.test(name) || /(?:^|[-_.])b(?:ack)?[-_.]?cu\.(?:gbr|ger)$/.test(name)) return gerberRole("copper-bottom", detectedBy);
  if (/\.(g\d+)$/.test(name) || /(?:^|[-_.])(?:in(?:ner)?[-_.]?\d+(?:[-_.]?cu)?|i\d+[-_.]?cu|cu[-_.]?\d+)\.(?:gbr|ger)$/.test(name)) return gerberRole("copper-inner", detectedBy);
  if (/\.(gts|stc)$/.test(name) || /(?:^|[-_.])f(?:ront)?[-_.]?(?:mask|soldermask)\.(?:gbr|ger)$/.test(name)) return gerberRole("mask-top", detectedBy);
  if (/\.(gbs|sts)$/.test(name) || /(?:^|[-_.])b(?:ack)?[-_.]?(?:mask|soldermask)\.(?:gbr|ger)$/.test(name)) return gerberRole("mask-bottom", detectedBy);
  if (/\.(gto|plc)$/.test(name) || /(?:^|[-_.])f(?:ront)?[-_.]?(?:silks?|silkscreen|legend)\.(?:gbr|ger)$/.test(name)) return gerberRole("silk-top", detectedBy);
  if (/\.(gbo|pls)$/.test(name) || /(?:^|[-_.])b(?:ack)?[-_.]?(?:silks?|silkscreen|legend)\.(?:gbr|ger)$/.test(name)) return gerberRole("silk-bottom", detectedBy);
  if (/\.(gtp|crc)$/.test(name) || /(?:^|[-_.])f(?:ront)?[-_.]?paste\.(?:gbr|ger)$/.test(name)) return gerberRole("paste-top", detectedBy);
  if (/\.(gbp|crs)$/.test(name) || /(?:^|[-_.])b(?:ack)?[-_.]?paste\.(?:gbr|ger)$/.test(name)) return gerberRole("paste-bottom", detectedBy);
  if (/\.(gko|gml|oln|dim|gbroutline)$/.test(name) || /(?:edge[-_.]?cuts|board[-_.]?(?:outline|profile)|outline|profile|contour)/.test(name)) return gerberRole("outline", detectedBy);
  if (/(?:drill[-_.]?(?:map|drawing)|fab(?:rication)?[-_.]?drawing|assembly[-_.]?drawing)/.test(name)) {
    return gerberRole(name.includes("drill") ? "drill-drawing" : "documentation", detectedBy);
  }
  return gerberRole("unknown", "unresolved");
}

function forms(source, name) {
  const output = [];
  const needle = `(${name}`;
  let cursor = 0;
  while ((cursor = source.indexOf(needle, cursor)) !== -1) {
    const boundary = source[cursor + needle.length];
    if (boundary && !/[\s)]/.test(boundary)) {
      cursor += needle.length;
      continue;
    }
    let depth = 0;
    let quoted = false;
    let escaped = false;
    for (let index = cursor; index < source.length; index += 1) {
      const char = source[index];
      if (quoted) {
        if (escaped) escaped = false;
        else if (char === "\\") escaped = true;
        else if (char === '"') quoted = false;
      } else if (char === '"') quoted = true;
      else if (char === "(") depth += 1;
      else if (char === ")" && --depth === 0) {
        output.push(source.slice(cursor, index + 1));
        cursor = index + 1;
        break;
      }
    }
    if (depth) break;
  }
  return output;
}

function numbers(form, field, count = 2) {
  const match = form.match(new RegExp(`\\(${field}\\s+(${Array(count).fill("-?[\\d.]+").join("\\s+")})`));
  return match ? match[1].trim().split(/\s+/).map(Number) : null;
}

function directNumbers(form, field, count = 2) {
  const needle = `(${field}`;
  let depth = 0;
  let quoted = false;
  let escaped = false;
  for (let index = 0; index < form.length; index += 1) {
    const char = form[index];
    if (quoted) {
      if (escaped) escaped = false;
      else if (char === "\\") escaped = true;
      else if (char === '"') quoted = false;
    } else if (char === '"') quoted = true;
    else if (char === "(") {
      if (depth === 1 && form.startsWith(needle, index) && /[\s)]/.test(form[index + needle.length] || ")")) {
        const match = form.slice(index).match(new RegExp(`^\\(${field}\\s+(${Array(count).fill("-?[\\d.]+").join("\\s+")})`));
        return match ? match[1].trim().split(/\s+/).map(Number) : null;
      }
      depth += 1;
    } else if (char === ")") depth -= 1;
  }
  return null;
}

function value(form, field) {
  const match = form.match(new RegExp(`\\(${field}\\s+(-?[\\d.]+)`));
  return match ? Number(match[1]) : null;
}

function layerName(form) {
  return form.match(/\(layer\s+"([^"]+)"/)?.[1] || "Other";
}

function addLayer(map, id, label, color) {
  if (!map.has(id)) map.set(id, { id, label, color, visible: true, strokes: [], flashes: [], polygons: [], labels: [] });
  return map.get(id);
}

function copperLayer(map, layer) {
  if (layer === "F.Cu" || layer.includes("*.Cu")) return addLayer(map, "front", "F.Cu", COLORS.front);
  if (layer === "B.Cu") return addLayer(map, "back", "B.Cu", COLORS.back);
  return addLayer(map, layer, layer, COLORS.other);
}

function finishModel(format, layers, warnings = [], fitLayer = null) {
  let primitives = 0;
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  const include = (x, y) => {
    minX = Math.min(minX, x); minY = Math.min(minY, y);
    maxX = Math.max(maxX, x); maxY = Math.max(maxY, y);
  };
  const collectBounds = (selectedLayers) => {
    minX = Infinity; minY = Infinity; maxX = -Infinity; maxY = -Infinity;
    for (const layer of selectedLayers) {
      for (const item of layer.strokes) { include(item.x1, item.y1); include(item.x2, item.y2); }
      for (const item of layer.flashes) {
        if (item.boundsOffset) {
          include(item.x + item.boundsOffset.minX, item.y + item.boundsOffset.minY);
          include(item.x + item.boundsOffset.maxX, item.y + item.boundsOffset.maxY);
        } else {
          include(item.x - item.w / 2, item.y - item.h / 2);
          include(item.x + item.w / 2, item.y + item.h / 2);
        }
      }
      for (const polygon of layer.polygons || []) for (const contour of polygon.contours) for (const point of contour) include(point.x, point.y);
      for (const item of layer.labels) include(item.x, item.y);
    }
  };
  for (const layer of layers) {
    primitives += layer.strokes.length + layer.flashes.reduce((sum, flash) => sum + (flash.primitiveWeight || 1), 0) + layer.labels.length +
      (layer.polygons || []).reduce((sum, polygon) => sum + polygon.contours.reduce((count, contour) => count + Math.max(1, contour.length - 1), 0), 0);
    if (primitives > 100_000) throw new Error(`${format} has more than 100,000 drawable primitives.`);
  }
  collectBounds(fitLayer ? [fitLayer] : layers);
  if (fitLayer && (minX === maxX || minY === maxY)) collectBounds(layers);
  if (!Number.isFinite(minX)) throw new Error(`No drawable ${format} geometry was found.`);
  const bounds = { minX, minY, maxX, maxY };
  bounds.width = bounds.maxX - bounds.minX || 1;
  bounds.height = bounds.maxY - bounds.minY || 1;
  return { format, layers, bounds, primitives, warnings: [...new Set(warnings)] };
}

function emptyModel(format, layers, warnings = []) {
  return {
    format,
    layers,
    bounds: { minX: 0, minY: 0, maxX: 1, maxY: 1, width: 1, height: 1 },
    primitives: 0,
    warnings: [...new Set(warnings)],
  };
}

export function parseKiCadView(source) {
  if (typeof source !== "string" || !/^\s*\(kicad_pcb\b/.test(source)) throw new Error("Not a KiCad board.");
  const layers = new Map();
  const edge = addLayer(layers, "edge", "Edge.Cuts", COLORS.edge);
  const components = addLayer(layers, "components", "Components", COLORS.silk);
  const warnings = [];

  for (const form of forms(source, "segment")) {
    const start = numbers(form, "start");
    const end = numbers(form, "end");
    if (start && end) copperLayer(layers, layerName(form)).strokes.push({ x1: start[0], y1: start[1], x2: end[0], y2: end[1], width: value(form, "width") || 0.2 });
  }
  for (const form of forms(source, "via")) {
    const at = numbers(form, "at");
    if (!at) continue;
    const size = value(form, "size") || 0.6;
    const drill = value(form, "drill") || 0;
    const span = /\(layers\s+"([^"]+)"\s+"([^"]+)"\)/.exec(form);
    if (!span || span[1] === "F.Cu" || span[2] === "F.Cu") copperLayer(layers, "F.Cu").flashes.push({ x: at[0], y: at[1], w: size, h: size, shape: "circle", drill });
    if (!span || span[1] === "B.Cu" || span[2] === "B.Cu") copperLayer(layers, "B.Cu").flashes.push({ x: at[0], y: at[1], w: size, h: size, shape: "circle", drill });
  }
  for (const name of ["gr_line", "gr_rect"]) {
    for (const form of forms(source, name)) {
      if (layerName(form) !== "Edge.Cuts") continue;
      const start = numbers(form, "start");
      const end = numbers(form, "end");
      if (!start || !end) continue;
      const line = (x1, y1, x2, y2) => edge.strokes.push({ x1, y1, x2, y2, width: 0.08 });
      if (name === "gr_rect") {
        line(start[0], start[1], end[0], start[1]); line(end[0], start[1], end[0], end[1]);
        line(end[0], end[1], start[0], end[1]); line(start[0], end[1], start[0], start[1]);
      } else line(start[0], start[1], end[0], end[1]);
    }
  }

  const footprintForms = [...forms(source, "footprint"), ...forms(source, "module")];
  for (const footprint of footprintForms) {
    const at = directNumbers(footprint, "at", 3) || [...(directNumbers(footprint, "at") || []), 0];
    if (at.length < 2) continue;
    const rotation = ((at[2] || 0) * Math.PI) / 180;
    const cos = Math.cos(rotation);
    const sin = Math.sin(rotation);
    const reference = footprint.match(/\(property\s+"Reference"\s+"([^"]+)"/)?.[1]
      || footprint.match(/\(fp_text\s+reference\s+"([^"]+)"/)?.[1]
      || "";
    components.labels.push({ x: at[0], y: at[1], text: reference });
    for (const pad of forms(footprint, "pad")) {
      const local = numbers(pad, "at", 3) || [...(numbers(pad, "at") || []), 0];
      const size = numbers(pad, "size");
      if (local.length < 2 || !size) continue;
      const x = at[0] + local[0] * cos - local[1] * sin;
      const y = at[1] + local[0] * sin + local[1] * cos;
      const padLayer = /\(layers\s+([^)]*)\)/.exec(pad)?.[1] || layerName(footprint);
      const shape = /^\(pad\s+"[^"]*"\s+\S+\s+(circle|oval)/.exec(pad)?.[1] || "rect";
      const flash = { x, y, w: size[0], h: size[1], shape, drill: value(pad, "drill") || 0, rotation: rotation + ((local[2] || 0) * Math.PI) / 180 };
      if (/F\.Cu|\*\.Cu/.test(padLayer)) copperLayer(layers, "F.Cu").flashes.push(flash);
      if (/B\.Cu|\*\.Cu/.test(padLayer)) copperLayer(layers, "B.Cu").flashes.push(flash);
    }
  }
  if (forms(source, "gr_arc").some((form) => layerName(form) === "Edge.Cuts")) warnings.push("Curved Edge.Cuts are not drawn in this lightweight board view.");
  if (!edge.strokes.length) warnings.push("No straight or rectangular Edge.Cuts were drawable.");
  return finishModel("KiCad layout", [...layers.values()].filter((layer) => layer.strokes.length || layer.flashes.length || layer.labels.length), warnings, edge.strokes.length ? edge : null);
}

export function parseGerber(source, filename = "layer.gbr", options = {}) {
  if (typeof source !== "string" || source.length < 10) throw new Error(`${filename} is empty or unreadable.`);
  let commandSeparators = 0;
  for (let index = 0; index < source.length; index += 1) {
    if (source.charCodeAt(index) === 42 && ++commandSeparators > 200_000) {
      throw new Error(`${filename} exceeds the 200,000-command parser safety limit.`);
    }
  }
  const terminalSource = source.match(/(?:^|\*)\s*M02\*([\s\S]*)$/i);
  if (terminalSource && terminalSource[1].trim()) {
    throw new Error(`${filename} contains data after its terminal M02 command.`);
  }
  const warnings = [];
  const fs = source.match(/%FS([LT])([AI])X(\d)(\d)Y(\d)(\d)\*%/i);
  const fsStarts = source.match(/%FS/gi)?.length || 0;
  const fsDefinitions = [...source.matchAll(/%FS[LT][AI]X\d\dY\d\d\*%/gi)].length;
  if ((fsStarts && fsDefinitions !== fsStarts) || fsDefinitions > 1) throw new Error(`${filename} has malformed or conflicting coordinate-format declarations.`);
  const zero = fs?.[1]?.toUpperCase() || "L";
  const absolute = (fs?.[2] || "A").toUpperCase() === "A";
  const xFormat = [Number(fs?.[3] || 2), Number(fs?.[4] || 4)];
  const yFormat = [Number(fs?.[5] || 2), Number(fs?.[6] || 4)];
  const unitStarts = source.match(/%MO/gi)?.length || 0;
  const unitDefinitions = [...source.matchAll(/%MO(?:IN|MM)\*%/gi)];
  if ((unitStarts && unitDefinitions.length !== unitStarts) || unitDefinitions.length > 1) throw new Error(`${filename} has malformed or conflicting unit declarations.`);
  const scale = /%MOIN\*%/i.test(source) ? 25.4 : 1;
  if (!fs) warnings.push("Coordinate format missing; assumed leading-zero 2.4.");
  if (!/%MO(?:IN|MM)\*%/i.test(source)) warnings.push("Units missing; assumed millimetres.");
  if (!absolute) throw new Error(`${filename} uses unsupported incremental coordinates.`);
  if (/%TF\.FilePolarity,\s*Negative(?:,[^*%]*)?\*%/i.test(source) || /%IPNEG\*%/i.test(source)) {
    throw new Error(`${filename} uses unsupported negative file or image polarity.`);
  }
  for (const polarity of source.matchAll(/%TF\.FilePolarity,([^*%]+)\*%/gi)) {
    if (!/^\s*Positive\s*$/i.test(polarity[1])) throw new Error(`${filename} has an unsupported file-polarity attribute.`);
  }
  const macroPattern = /%AM([^*%]+)\*([\s\S]*?)\*%/gi;
  const macroBlocks = [...source.matchAll(macroPattern)];
  const macroStarts = source.match(/%AM/gi)?.length || 0;
  let roundRectMacro = false;
  if (macroStarts !== macroBlocks.length) throw new Error(`${filename} contains a malformed aperture macro block.`);
  for (const block of macroBlocks) {
    if (block[1] !== "RoundRect" || roundRectMacro) throw new Error(`${filename} uses unsupported aperture macros.`);
    const executable = block[2]
      .split("*")
      .map((line) => line.trim())
      .filter(Boolean)
      .filter((line) => !/^0(?:[\s,]|$)/.test(line))
      .map((line) => line.replace(/\s+/g, ""));
    if (executable.length !== KICAD_ROUNDRECT_MACRO.length ||
        executable.some((line, index) => line !== KICAD_ROUNDRECT_MACRO[index])) {
      throw new Error(`${filename} uses an unsupported or modified RoundRect aperture macro.`);
    }
    roundRectMacro = true;
  }
  if (/%ABD\d+\*%/i.test(source)) throw new Error(`${filename} uses unsupported aperture blocks.`);
  if (/%ADD\d+P/i.test(source)) throw new Error(`${filename} uses unsupported polygon apertures.`);
  if (/%(?:LM|LR|LS|SR)/i.test(source)) throw new Error(`${filename} uses unsupported transforms or step-repeat.`);
  if (/%(?:OF|SF|AS|MI|IR)[^*%]*\*%/i.test(source)) throw new Error(`${filename} uses unsupported legacy offsets, scaling, axis mapping, mirroring, or rotation.`);

  const decode = (raw, [integer, decimal]) => {
    if (raw === undefined) return null;
    const sign = raw.startsWith("-") ? -1 : 1;
    let digits = raw.replace(/^[+-]/, "");
    if (digits.includes(".")) {
      if (digits.length > 32) throw new Error(`${filename} contains an overlong coordinate.`);
      const explicit = sign * Number(digits) * scale;
      if (!Number.isFinite(explicit)) throw new Error(`${filename} contains a non-finite coordinate.`);
      return explicit;
    }
    if (digits.length > integer + decimal) throw new Error(`${filename} contains a coordinate wider than its declared format.`);
    digits = zero === "T" ? digits.padEnd(integer + decimal, "0") : digits.padStart(integer + decimal, "0");
    const decoded = sign * Number(digits) / 10 ** decimal * scale;
    if (!Number.isFinite(decoded)) throw new Error(`${filename} contains a non-finite coordinate.`);
    return decoded;
  };

  const apertures = new Map();
  for (const match of source.matchAll(/%ADD(\d+)([CROP])(?:,([^*%]*))?\*%/gi)) {
    const rawParams = (match[3] || "").trim();
    const parts = rawParams ? rawParams.split(/[Xx]/).map((part) => part.trim()) : [];
    const params = parts.map(Number);
    const shape = match[2].toUpperCase();
    if ((shape === "C" && params.length > 1) || (["R", "O"].includes(shape) && params.length > 2)) {
      throw new Error(`${filename} uses unsupported aperture holes.`);
    }
    const expectedParams = shape === "C" ? 1 : 2;
    if (parts.length !== expectedParams || parts.some((part) => !part) || params.some((param) => !Number.isFinite(param) || param <= 0)) {
      throw new Error(`${filename} has an invalid aperture definition; dimensions must be positive numbers.`);
    }
    if (apertures.has(match[1])) throw new Error(`${filename} defines aperture D${match[1]} more than once.`);
    apertures.set(match[1], { shape, w: params[0] * scale, h: (params[1] ?? params[0]) * scale });
  }
  for (const match of source.matchAll(/%ADD(\d+)RoundRect,([^*%]+)\*%/g)) {
    if (!roundRectMacro) throw new Error(`${filename} defines RoundRect without the validated KiCad macro.`);
    const parts = match[2].split("X").map((part) => part.trim());
    if (parts.length !== 10 || parts.some((part) => !/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)$/.test(part))) {
      throw new Error(`${filename} has invalid RoundRect aperture modifiers.`);
    }
    const values = parts.map(Number);
    if (values.some((value) => !Number.isFinite(value) || Math.abs(value * scale) > 1_000) || values[0] <= 0 || values[9] !== 0) {
      throw new Error(`${filename} has invalid RoundRect aperture dimensions.`);
    }
    const radius = values[0] * scale;
    const points = [
      { x: values[1] * scale, y: -values[2] * scale },
      { x: values[3] * scale, y: -values[4] * scale },
      { x: values[5] * scale, y: -values[6] * scale },
      { x: values[7] * scale, y: -values[8] * scale },
    ];
    const turns = points.map((point, index) => {
      const next = points[(index + 1) % points.length];
      const after = points[(index + 2) % points.length];
      return (next.x - point.x) * (after.y - next.y) - (next.y - point.y) * (after.x - next.x);
    });
    const direction = Math.sign(turns[0]);
    const edges = points.map((point, index) => Math.hypot(
      points[(index + 1) % points.length].x - point.x,
      points[(index + 1) % points.length].y - point.y,
    ));
    if (!direction || turns.some((turn) => Math.sign(turn) !== direction) || edges.some((length) => length <= 1e-9)) {
      throw new Error(`${filename} has a non-convex or degenerate RoundRect aperture.`);
    }
    if (apertures.has(match[1])) throw new Error(`${filename} defines aperture D${match[1]} more than once.`);
    const xs = points.map(({ x }) => x);
    const ys = points.map(({ y }) => y);
    apertures.set(match[1], {
      shape: "RoundRect",
      radius,
      points,
      w: Math.max(...xs) - Math.min(...xs) + radius * 2,
      h: Math.max(...ys) - Math.min(...ys) + radius * 2,
      boundsOffset: {
        minX: Math.min(...xs) - radius,
        minY: Math.min(...ys) - radius,
        maxX: Math.max(...xs) + radius,
        maxY: Math.max(...ys) + radius,
      },
      primitiveWeight: 9,
    });
  }
  for (const match of source.matchAll(/%ADD(\d+)([^,*%]+)[^*%]*\*%/gi)) {
    if (!apertures.has(match[1])) throw new Error(`${filename} uses unsupported aperture D${match[1]} (${match[2]}).`);
  }
  const role = classifyGerberLayer(filename, source);
  const layer = {
    id: `${role.key}:${filename}`,
    label: filename,
    filename,
    color: role.color,
    visible: true,
    role,
    parseWarnings: warnings,
    strokes: [],
    flashes: [],
    polygons: [],
    labels: [],
  };
  let x = null;
  let y = null;
  let aperture = null;
  let interpolation = null;
  let multiQuadrant = false;
  let modalOperation = null;
  let polarity = "dark";
  let renderOrder = 0;
  let allocatedPrimitives = 0;
  const requestedPrimitiveLimit = options.maxPrimitives === undefined ? 100_000 : Number(options.maxPrimitives);
  if (!Number.isFinite(requestedPrimitiveLimit) || requestedPrimitiveLimit < 1) {
    throw new Error(`${filename} cannot be parsed because the aggregate Gerber primitive budget is exhausted.`);
  }
  const maximumPrimitives = Math.min(100_000, Math.floor(requestedPrimitiveLimit));
  const addPrimitive = (collection, primitive, weight = 1) => {
    allocatedPrimitives += weight;
    if (allocatedPrimitives > maximumPrimitives) {
      throw new Error(`${filename} exceeds the ${maximumPrimitives.toLocaleString()}-primitive viewer safety limit.`);
    }
    collection.push(primitive);
  };
  const reservePrimitive = () => {
    allocatedPrimitives += 1;
    if (allocatedPrimitives > maximumPrimitives) {
      throw new Error(`${filename} exceeds the ${maximumPrimitives.toLocaleString()}-primitive viewer safety limit.`);
    }
  };
  let region = null;
  let topologyAssignments = 0;
  let topologyComparisons = 0;
  let cutInRegions = false;
  let quantizedRegionSpurs = false;
  const samePoint = (left, right) => Math.hypot(left.x - right.x, left.y - right.y) <= 1e-9;
  const orientation = (a, b, c) => (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
  const onSegment = (a, b, point) =>
    Math.abs(orientation(a, b, point)) <= 1e-10 &&
    point.x >= Math.min(a.x, b.x) - 1e-10 && point.x <= Math.max(a.x, b.x) + 1e-10 &&
    point.y >= Math.min(a.y, b.y) - 1e-10 && point.y <= Math.max(a.y, b.y) + 1e-10;
  const segmentsIntersect = (a, b, c, d) => {
    const abC = orientation(a, b, c);
    const abD = orientation(a, b, d);
    const cdA = orientation(c, d, a);
    const cdB = orientation(c, d, b);
    if (((abC > 0 && abD < 0) || (abC < 0 && abD > 0)) &&
        ((cdA > 0 && cdB < 0) || (cdA < 0 && cdB > 0))) return true;
    return onSegment(a, b, c) || onSegment(a, b, d) || onSegment(c, d, a) || onSegment(c, d, b);
  };
  const validateRegionContour = (contour) => {
    if (contour.length < 4 || !samePoint(contour[0], contour.at(-1))) {
      throw new Error(`${filename} contains a region contour that is not explicitly closed.`);
    }
    const rawPoints = contour.slice(0, -1);
    for (let index = 0; index < rawPoints.length; index += 1) {
      if (samePoint(rawPoints[index], rawPoints[(index + 1) % rawPoints.length])) {
        throw new Error(`${filename} contains a zero-length region edge.`);
      }
    }
    const coordinateQuantum = Math.max(10 ** -xFormat[1], 10 ** -yFormat[1]) * scale;
    const topologyPoints = [];
    for (const point of rawPoints) {
      if (topologyPoints.length && Math.hypot(point.x - topologyPoints.at(-1).x, point.y - topologyPoints.at(-1).y) <= coordinateQuantum * (1 + 1e-9)) {
        quantizedRegionSpurs = true;
        continue;
      }
      topologyPoints.push(point);
    }
    if (topologyPoints.length > 3 &&
        Math.hypot(topologyPoints.at(-1).x - topologyPoints[0].x, topologyPoints.at(-1).y - topologyPoints[0].y) <= coordinateQuantum * (1 + 1e-9)) {
      topologyPoints.pop();
      quantizedRegionSpurs = true;
    }
    const pointKey = (point) => `${point.x}\0${point.y}`;
    const occurrences = topologyPoints.reduce((counts, point) => {
      const key = pointKey(point);
      counts.set(key, (counts.get(key) || 0) + 1);
      return counts;
    }, new Map());
    const points = [];
    for (const point of topologyPoints) {
      points.push(point);
      while (points.length >= 3 && orientation(points.at(-3), points.at(-2), points.at(-1)) === 0 &&
             occurrences.get(pointKey(points.at(-2))) === 1) {
        points.splice(points.length - 2, 1);
      }
    }
    let leadingCollinear = 0;
    while (points.length - leadingCollinear >= 3 &&
           orientation(points.at(-1), points[leadingCollinear], points[leadingCollinear + 1]) === 0 &&
           occurrences.get(pointKey(points[leadingCollinear])) === 1) {
      leadingCollinear += 1;
    }
    if (leadingCollinear) points.splice(0, leadingCollinear);
    while (points.length >= 3 && orientation(points.at(-2), points.at(-1), points[0]) === 0 &&
           occurrences.get(pointKey(points.at(-1))) === 1) points.pop();
    if (points.length < 3) throw new Error(`${filename} contains a zero-area region contour.`);
    const segments = points.map((point, index) => {
      const next = points[(index + 1) % points.length];
      return {
        index,
        a: point,
        b: next,
        minX: Math.min(point.x, next.x),
        minY: Math.min(point.y, next.y),
        maxX: Math.max(point.x, next.x),
        maxY: Math.max(point.y, next.y),
      };
    });
    const extent = segments.reduce((bounds, segment) => ({
      minX: Math.min(bounds.minX, segment.minX),
      minY: Math.min(bounds.minY, segment.minY),
      maxX: Math.max(bounds.maxX, segment.maxX),
      maxY: Math.max(bounds.maxY, segment.maxY),
    }), { minX: Infinity, minY: Infinity, maxX: -Infinity, maxY: -Infinity });
    const { minX, minY, maxX, maxY } = extent;
    const columns = Math.max(1, Math.min(128, Math.ceil(Math.sqrt(segments.length))));
    const width = maxX - minX || 1;
    const height = maxY - minY || 1;
    const cellFor = (value, minimum, span) => Math.max(0, Math.min(columns - 1, Math.floor(((value - minimum) / span) * columns)));
    const cells = Array.from({ length: columns * columns }, () => []);
    for (const segment of segments) {
      const left = cellFor(segment.minX, minX, width);
      const right = cellFor(segment.maxX, minX, width);
      const top = cellFor(segment.minY, minY, height);
      const bottom = cellFor(segment.maxY, minY, height);
      for (let row = top; row <= bottom; row += 1) {
        for (let column = left; column <= right; column += 1) {
          topologyAssignments += 1;
          if (topologyAssignments > 2_000_000) throw new Error(`${filename} contains regions too complex for safe topology validation.`);
          cells[row * columns + column].push(segment);
        }
      }
    }
    let selfTouching = false;
    const pointInside = (segment, point) =>
      onSegment(segment.a, segment.b, point) && !samePoint(segment.a, point) && !samePoint(segment.b, point);
    for (const candidates of cells) {
      for (let left = 0; left < candidates.length; left += 1) {
        const first = candidates[left];
        for (let right = left + 1; right < candidates.length; right += 1) {
          topologyComparisons += 1;
          if (topologyComparisons > 2_000_000) throw new Error(`${filename} contains regions too complex for safe topology validation.`);
          const second = candidates[right];
          const distance = Math.abs(first.index - second.index);
          if (distance === 1 || distance === segments.length - 1) continue;
          if (first.maxX < second.minX || second.maxX < first.minX || first.maxY < second.minY || second.maxY < first.minY) continue;
          if (!segmentsIntersect(first.a, first.b, second.a, second.b)) continue;
          const sameDirection = samePoint(first.a, second.a) && samePoint(first.b, second.b);
          const reverseDirection = samePoint(first.a, second.b) && samePoint(first.b, second.a);
          if (reverseDirection) {
            selfTouching = true;
            continue;
          }
          if (sameDirection || pointInside(first, second.a) || pointInside(first, second.b) ||
              pointInside(second, first.a) || pointInside(second, first.b)) {
            const edge = ({ a, b }) => `(${a.x},${a.y})→(${b.x},${b.y})`;
            throw new Error(`${filename} contains an overlapping region contour between edges ${first.index} ${edge(first)} and ${second.index} ${edge(second)}.`);
          }
          const sharedEndpoint = samePoint(first.a, second.a) || samePoint(first.a, second.b) ||
            samePoint(first.b, second.a) || samePoint(first.b, second.b);
          if (!sharedEndpoint) throw new Error(`${filename} contains a self-intersecting region contour between edges ${first.index} and ${second.index}.`);
          selfTouching = true;
        }
      }
    }
    const twiceArea = points.reduce((sum, point, index) => {
      const next = points[(index + 1) % points.length];
      return sum + point.x * next.y - next.x * point.y;
    }, 0);
    if (!Number.isFinite(twiceArea) || Math.abs(twiceArea) <= 1e-12) throw new Error(`${filename} contains a zero-area region contour.`);
    return { contour, selfTouching };
  };
  const closeRegionContour = () => {
    if (!region?.current?.length) return;
    const validated = validateRegionContour(region.current);
    cutInRegions ||= validated.selfTouching;
    region.contours.push(validated.contour);
    region.current = null;
  };
  const finishRegion = () => {
    if (!region) throw new Error(`${filename} ends a region that was not started.`);
    closeRegionContour();
    if (!region.contours.length) throw new Error(`${filename} contains an empty region.`);
    for (const contour of region.contours) layer.polygons.push({ contours: [contour], polarity: region.polarity, order: renderOrder++ });
    region = null;
    warnings.push("Filled regions were rendered from their bounded contours.");
  };
  const arcPoints = (startX, startY, endX, endY, iOffset, jOffset, direction) => {
    if (!multiQuadrant) throw new Error(`${filename} uses unsupported single-quadrant or unspecified arc interpolation.`);
    if (iOffset === null || jOffset === null) throw new Error(`${filename} contains an arc without explicit I and J centre offsets.`);
    const rawStartY = -startY;
    const rawEndY = -endY;
    const centerX = startX + (iOffset || 0);
    const centerY = rawStartY + (jOffset || 0);
    const startRadius = Math.hypot(startX - centerX, rawStartY - centerY);
    const endRadius = Math.hypot(endX - centerX, rawEndY - centerY);
    const radiusTolerance = Math.max(1e-9, 2 * 10 ** -Math.min(xFormat[1], yFormat[1]) * scale);
    if (!Number.isFinite(startRadius) || startRadius <= 0 || Math.abs(startRadius - endRadius) > radiusTolerance) {
      throw new Error(`${filename} contains an arc whose endpoint does not match its I/J radius.`);
    }
    const startAngle = Math.atan2(rawStartY - centerY, startX - centerX);
    const endAngle = Math.atan2(rawEndY - centerY, endX - centerX);
    let sweep = endAngle - startAngle;
    if (direction === "clockwise") {
      while (sweep >= -1e-12) sweep -= Math.PI * 2;
    } else {
      while (sweep <= 1e-12) sweep += Math.PI * 2;
    }
    const segments = Math.max(2, Math.ceil(Math.max(
      Math.abs(sweep) / (Math.PI / 36),
      startRadius * Math.abs(sweep) / 0.25,
    )));
    if (segments > 4_096) throw new Error(`${filename} contains an arc requiring too many display segments.`);
    const points = [];
    for (let index = 1; index <= segments; index += 1) {
      const angle = startAngle + (sweep * index) / segments;
      points.push({
        x: index === segments ? endX : centerX + startRadius * Math.cos(angle),
        y: index === segments ? endY : -(centerY + startRadius * Math.sin(angle)),
      });
    }
    warnings.push("Circular interpolation was approximated with bounded straight chords.");
    return points;
  };
  const flashAt = (flashX, flashY) => {
    if (!aperture) throw new Error(`${filename} draws or flashes before selecting a defined aperture.`);
    const shape = aperture.shape === "C" ? "circle" : aperture.shape === "O" ? "oval" : aperture.shape === "RoundRect" ? "roundrect-macro" : "rect";
    addPrimitive(layer.flashes, {
      x: flashX,
      y: flashY,
      w: aperture.w,
      h: aperture.h,
      shape,
      drill: 0,
      polarity,
      order: renderOrder++,
      ...(shape === "roundrect-macro" ? {
        points: aperture.points,
        radius: aperture.radius,
        boundsOffset: aperture.boundsOffset,
        primitiveWeight: aperture.primitiveWeight,
      } : {}),
    }, aperture.primitiveWeight || 1);
  };
  const withoutMacros = source.replace(/%AM([^*%]+)\*([\s\S]*?)\*%/gi, "");
  for (const block of withoutMacros.matchAll(/%([^%]*)%/g)) {
    const commandsInBlock = block[1].split("*").map((command) => command.trim()).filter(Boolean);
    for (const extended of commandsInBlock) {
      const allowed = /^(?:FS[LT][AI]X\d\dY\d\d|MO(?:IN|MM)|ADD\d+.+|LP[DC]|IPPOS)$/i.test(extended) ||
        /^(?:TF|TA|TO)\.[^\u0000-\u001f\u007f]+$/i.test(extended) ||
        /^TD(?:\.[^\u0000-\u001f\u007f,]+)?$/i.test(extended);
      if (!allowed) throw new Error(`${filename} uses an unsupported extended command: ${extended.slice(0, 80)}.`);
    }
  }
  const commandSource = withoutMacros.replace(/%([^%]*)%/g, (_block, body) => body
    .split("*")
    .map((command) => command.trim())
    .filter((command) => /^LP[DC]$/i.test(command))
    .map((command) => `${command}*`)
    .join(""));
  if (commandSource.includes("%")) throw new Error(`${filename} contains an unterminated extended command block.`);
  const commands = commandSource.split("*").map((part) => part.trim()).filter(Boolean);
  if (!apertures.size && commands.some((command) =>
    !/^G0?4(?!\d)/i.test(command) && (/^(?:G54)?D(?:0?[123]|\d{2,})$/i.test(command) || /[XY][+-]?(?:\d|\.)/i.test(command)))) {
    throw new Error(`${filename} has no valid aperture definitions.`);
  }
  const terminators = commands.map((command, index) => /^M02$/i.test(command) ? index : -1).filter((index) => index >= 0);
  if (commands.some((command) => /^M0[01]$/i.test(command))) {
    throw new Error(`${filename} uses unsupported M00/M01 image-stop commands.`);
  }
  if (terminators.length > 1 || (terminators.length === 1 && terminators[0] !== commands.length - 1)) {
    throw new Error(`${filename} must contain exactly one terminal M02 command with no following commands.`);
  }
  for (const command of commands) {
    if (/^G0?4(?!\d)/i.test(command)) continue;
    if (/^LP[DC]$/i.test(command)) {
      if (region) throw new Error(`${filename} changes polarity inside an open region.`);
      polarity = /^LPC$/i.test(command) ? "clear" : "dark";
      continue;
    }
    if (/^G(?:70|71|90|91)(?:$|(?=[XYIJD]))/i.test(command)) {
      throw new Error(`${filename} uses unsupported legacy unit or coordinate-mode commands.`);
    }
    if (/^G74$/i.test(command)) { multiQuadrant = false; continue; }
    if (/^G75$/i.test(command)) { multiQuadrant = true; continue; }
    if (/^G36$/i.test(command)) {
      if (region) throw new Error(`${filename} starts a nested region.`);
      region = { contours: [], current: null, polarity };
      continue;
    }
    if (/^G37$/i.test(command)) { finishRegion(); continue; }
    if (/^G0?1(?:$|(?=[XYIJD]))/i.test(command)) interpolation = "linear";
    if (/^G0?2(?:$|(?=[XYIJD]))/i.test(command)) interpolation = "clockwise";
    if (/^G0?3(?:$|(?=[XYIJD]))/i.test(command)) interpolation = "counterclockwise";
    const apertureCode = command.match(/(?:^|G54)D(\d+)$/i)?.[1];
    if (apertureCode && Number(apertureCode) >= 10) {
      aperture = apertures.get(apertureCode);
      if (!aperture) throw new Error(`${filename} selects undefined aperture D${apertureCode}.`);
      // Deprecated coordinate-only imaging is defined only after D01. An
      // aperture selection makes the old operation mode undefined.
      modalOperation = null;
      continue;
    }
    if (/^G0?[123]/i.test(command) || /[XYIJ]/i.test(command) || /D0?[123]$/i.test(command)) {
      const numericToken = "[+-]?(?:\\d+(?:\\.\\d*)?|\\.\\d+)";
      for (const axis of ["X", "Y", "I", "J"]) {
        const occurrences = command.match(new RegExp(axis, "gi"))?.length || 0;
        const tokens = command.match(new RegExp(`${axis}${numericToken}`, "gi"))?.length || 0;
        if (occurrences !== tokens || tokens > 1) throw new Error(`${filename} contains malformed or duplicate ${axis} coordinate tokens.`);
      }
      const residue = command
        .replace(/^G0?[123]/i, "")
        .replace(new RegExp(`[XYIJ]${numericToken}`, "gi"), "")
        .replace(/D0?[123]$/i, "");
      if (residue) throw new Error(`${filename} contains an unsupported standard command: ${command.slice(0, 80)}.`);
    }
    const hasCoordinates = /[XY]/i.test(command);
    const hasArcOffsets = /[IJ]/i.test(command);
    const carriesArcGeometry = hasArcOffsets && interpolation !== "linear";
    const explicitOperation = command.match(/D0?([123])(?:$|M)/i)?.[1] || null;
    const operation = explicitOperation || (hasCoordinates || carriesArcGeometry ? modalOperation : null);
    if (hasArcOffsets && (interpolation === "linear" || operation !== "1")) {
      throw new Error(`${filename} contains I/J offsets outside an interpolated D01 arc.`);
    }
    if (!operation) {
      if (/^(?:G0?[123]|M02)$/i.test(command)) continue;
      throw new Error(`${filename} uses an unsupported standard command: ${command.slice(0, 80)}.`);
    }
    // D02 and D03 never establish a modal operation for a following
    // coordinate-only command. D01 does, including a coordinate-less
    // zero-length plot at the current point.
    if (explicitOperation) modalOperation = explicitOperation === "1" ? "1" : null;
    if (!hasCoordinates && !hasArcOffsets) {
      if (operation === "1") {
        if (region) throw new Error(`${filename} contains a zero-length D01 inside a region.`);
        if (!interpolation) throw new Error(`${filename} uses D01 before declaring G01, G02, or G03 plot mode.`);
        if (interpolation !== "linear") throw new Error(`${filename} contains a circular D01 without explicit I and J centre offsets.`);
        if (x === null || y === null) throw new Error(`${filename} images at an undefined current point.`);
        flashAt(x, y);
      } else if (operation === "2" && region) {
        if (x === null || y === null) throw new Error(`${filename} starts a region contour at an undefined current point.`);
        closeRegionContour();
        region.current = [{ x, y }];
      } else if (operation === "3") {
        if (region) throw new Error(`${filename} flashes an aperture inside a region.`);
        if (x === null || y === null) throw new Error(`${filename} images at an undefined current point.`);
        flashAt(x, y);
      }
      continue;
    }
    if ((hasCoordinates || carriesArcGeometry) && (operation === "1" || operation === "3") && !aperture && !(region && operation === "1")) {
      throw new Error(`${filename} draws or flashes before selecting a defined aperture.`);
    }
    const nextX = decode(command.match(/X([+-]?[\d.]+)/i)?.[1], xFormat);
    const nextY = decode(command.match(/Y([+-]?[\d.]+)/i)?.[1], yFormat);
    const nx = nextX ?? x;
    const ny = nextY === null ? y : -nextY;
    if ((hasCoordinates || carriesArcGeometry) && (nx === null || ny === null)) {
      throw new Error(`${filename} omits an axis before the current point is fully defined.`);
    }
    if (region && operation === "2" && hasCoordinates) {
      closeRegionContour();
      region.current = [{ x: nx, y: ny }];
    } else if (region && operation === "3") {
      throw new Error(`${filename} flashes an aperture inside a region.`);
    } else if (operation === "1" && (hasCoordinates || carriesArcGeometry)) {
      if (!interpolation) throw new Error(`${filename} uses D01 before declaring G01, G02, or G03 plot mode.`);
      if (region && !region.current) throw new Error(`${filename} starts a region contour without an explicit D02 move.`);
      if (x === null || y === null) throw new Error(`${filename} draws from an undefined current point; use D02 to establish it first.`);
      if (region && interpolation !== "linear") throw new Error(`${filename} uses an arc inside a region; this bounded viewer accepts linear region contours only.`);
      if (!region && aperture.shape !== "C") {
        throw new Error(`${filename} strokes with a non-circular aperture; that Minkowski geometry is not supported by this bounded viewer.`);
      }
      const points = interpolation === "linear"
        ? [{ x: nx, y: ny }]
        : arcPoints(
            x,
            y,
            nx,
            ny,
            decode(command.match(/I([+-]?[\d.]+)/i)?.[1], xFormat),
            decode(command.match(/J([+-]?[\d.]+)/i)?.[1], yFormat),
            interpolation,
          );
      if (region) {
        for (const point of points) {
          reservePrimitive();
          region.current.push(point);
        }
      } else {
        let previous = { x, y };
        for (const point of points) {
          addPrimitive(layer.strokes, {
            x1: previous.x,
            y1: previous.y,
            x2: point.x,
            y2: point.y,
            width: aperture.w,
            polarity,
            order: renderOrder++,
          });
          previous = point;
        }
      }
    }
    if (operation === "3" && hasCoordinates) flashAt(nx, ny);
    x = nx;
    y = ny;
  }
  if (region) throw new Error(`${filename} contains an unterminated region.`);
  if (!terminators.length) throw new Error(`${filename} must contain exactly one terminal M02 command with no following commands.`);
  if (cutInRegions) warnings.push("Self-touching cut-in regions were validated and rendered with even-odd fill.");
  if (quantizedRegionSpurs) warnings.push("Coordinate-quantized zero-area region spurs were normalized for topology validation.");
  if ([...layer.strokes, ...layer.flashes, ...layer.polygons].some(({ polarity: itemPolarity }) => itemPolarity === "clear")) {
    warnings.push("Clear-polarity geometry was composited in source order within its layer.");
  }
  if (!allocatedPrimitives) {
    warnings.push("The Gerber layer is valid but contains no drawable geometry.");
  }
  const uniqueWarnings = [...new Set(warnings)];
  layer.parseWarnings = uniqueWarnings;
  return allocatedPrimitives
    ? finishModel("Gerber RS-274X", [layer], uniqueWarnings)
    : emptyModel("Gerber RS-274X", [layer], uniqueWarnings);
}

function primitiveCount(layer) {
  return layer.strokes.length + layer.flashes.reduce((sum, flash) => sum + (flash.primitiveWeight || 1), 0) + layer.labels.length +
    (layer.polygons || []).reduce((sum, polygon) => sum + polygon.contours.reduce((count, contour) => count + Math.max(1, contour.length - 1), 0), 0);
}

function boundsForLayer(layer) {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  const include = (x, y) => {
    minX = Math.min(minX, x); minY = Math.min(minY, y);
    maxX = Math.max(maxX, x); maxY = Math.max(maxY, y);
  };
  for (const item of layer.strokes) { include(item.x1, item.y1); include(item.x2, item.y2); }
  for (const item of layer.flashes) {
    if (item.boundsOffset) {
      include(item.x + item.boundsOffset.minX, item.y + item.boundsOffset.minY);
      include(item.x + item.boundsOffset.maxX, item.y + item.boundsOffset.maxY);
    } else {
      include(item.x - item.w / 2, item.y - item.h / 2);
      include(item.x + item.w / 2, item.y + item.h / 2);
    }
  }
  for (const polygon of layer.polygons || []) for (const contour of polygon.contours) for (const point of contour) include(point.x, point.y);
  for (const item of layer.labels) include(item.x, item.y);
  return Number.isFinite(minX) ? { minX, minY, maxX, maxY } : null;
}

function unionBounds(bounds) {
  const usable = bounds.filter(Boolean);
  if (!usable.length) return null;
  return usable.reduce((total, box) => ({
    minX: Math.min(total.minX, box.minX),
    minY: Math.min(total.minY, box.minY),
    maxX: Math.max(total.maxX, box.maxX),
    maxY: Math.max(total.maxY, box.maxY),
  }));
}

function boundsOverlap(a, b) {
  return a && b && a.minX <= b.maxX && a.maxX >= b.minX && a.minY <= b.maxY && a.maxY >= b.minY;
}

function check(id, label, status, evidence) {
  return { id, label, status, evidence };
}

/** Build the evidence ledger shown beside the Gerber canvas. */
export function inspectGerberSet(model, { failures = [] } = {}) {
  const loaded = model.layers.map((layer) => ({
    id: layer.id,
    filename: layer.filename || layer.label,
    role: layer.role || classifyGerberLayer(layer.filename || layer.label),
    primitiveCount: primitiveCount(layer),
    status: primitiveCount(layer) ? "rendered" : "empty",
    warnings: [...(layer.parseWarnings || [])],
    bounds: boundsForLayer(layer),
  }));
  const rejected = failures.map((failure, index) => {
    const filename = typeof failure === "string" ? failure.split(":")[0] : failure.filename;
    const reason = typeof failure === "string" ? failure : failure.error;
    return {
      id: `rejected:${index}:${filename}`,
      filename,
      role: classifyGerberLayer(filename),
      primitiveCount: 0,
      status: "rejected",
      reason,
      warnings: [],
      bounds: null,
    };
  });
  const inventory = [...loaded, ...rejected].sort((a, b) => a.role.order - b.role.order || a.filename.localeCompare(b.filename));
  const drawable = loaded.filter((item) => item.status === "rendered");
  const empty = loaded.filter((item) => item.status === "empty");
  const renderedRoles = (key) => drawable.filter((item) => item.role.key === key);
  const inventoryRoles = (key) => inventory.filter((item) => item.role.key === key);
  const roleEvidence = (key, label) => {
    const candidates = inventoryRoles(key);
    const rendered = candidates.filter(({ status }) => status === "rendered");
    const validEmpty = candidates.filter(({ status }) => status === "empty");
    const rejectedCandidates = candidates.filter(({ status }) => status === "rejected");
    if (rendered.length === 1 && candidates.length === 1) return `${label} found`;
    if (rendered.length) {
      return `${label} has ${rendered.length} drawable candidate${rendered.length === 1 ? "" : "s"}` +
        `${validEmpty.length ? ` and ${validEmpty.length} valid-empty candidate${validEmpty.length === 1 ? "" : "s"}` : ""}` +
        `${rejectedCandidates.length ? ` and ${rejectedCandidates.length} rejected candidate${rejectedCandidates.length === 1 ? "" : "s"}` : ""}`;
    }
    if (validEmpty.length) return `${label} is present but valid empty`;
    if (rejectedCandidates.length) return `${label} is present but rejected by the parser`;
    return `${label} missing`;
  };
  const parserWarnings = loaded.flatMap((item) => item.warnings.map((warning) => `${item.filename}: ${warning}`));
  const checks = [];
  const total = inventory.length;

  checks.push(check(
    "visual-parse",
    "Visual geometry parse",
    rejected.length || parserWarnings.length ? "warning" : "pass",
    `${loaded.length}/${total} file${total === 1 ? "" : "s"} parsed; ${drawable.length} contain drawable geometry and ${empty.length} ${empty.length === 1 ? "is a valid empty layer" : "are valid empty layers"}. ${model.primitives} bounded draw/flash/region primitive${model.primitives === 1 ? "" : "s"}.${rejected.length ? ` ${rejected.length} rejected.` : ""}${parserWarnings.length ? ` ${parserWarnings.length} parser note${parserWarnings.length === 1 ? "" : "s"}.` : ""}`,
  ));

  const profiles = renderedRoles("outline");
  const profileCandidates = inventoryRoles("outline");
  const rejectedProfiles = profileCandidates.filter(({ status }) => status === "rejected");
  const emptyProfiles = profileCandidates.filter(({ status }) => status === "empty");
  const unambiguousProfile = profiles.length === 1 && profileCandidates.length === 1;
  checks.push(check(
    "board-profile",
    "Board profile identified",
    unambiguousProfile ? "pass" : "warning",
    unambiguousProfile
      ? `${profiles[0].filename} is the single rendered profile candidate.`
      : profiles.length
        ? `${profiles.length} profile candidate${profiles.length === 1 ? "" : "s"} rendered${emptyProfiles.length ? `, ${emptyProfiles.length} additional candidate${emptyProfiles.length === 1 ? " is" : "s are"} valid but empty` : ""}${rejectedProfiles.length ? `, and ${rejectedProfiles.length} additional candidate${rejectedProfiles.length === 1 ? " was" : "s were"} rejected` : ""}; confirm which contour drives routing.`
        : emptyProfiles.length
          ? `${emptyProfiles.length} profile candidate${emptyProfiles.length === 1 ? " is" : "s are"} syntactically valid but contain no drawable routing contour.`
        : rejectedProfiles.length
          ? `${rejectedProfiles.length} profile candidate${rejectedProfiles.length === 1 ? " was" : "s were"} identified but rejected by the parser.`
          : "No outline/profile layer was identified by X2 metadata or filename.",
  ));

  const topCopper = renderedRoles("copper-top");
  const bottomCopper = renderedRoles("copper-bottom");
  const innerCopper = renderedRoles("copper-inner");
  const copperComplete = topCopper.length === 1 && bottomCopper.length === 1;
  checks.push(check(
    "copper-stack",
    "Copper stack coverage",
    copperComplete ? "pass" : "warning",
    copperComplete
      ? `Top and bottom copper rendered${innerCopper.length ? ` with ${innerCopper.length} inner layer${innerCopper.length === 1 ? "" : "s"}` : ""}.`
      : `${roleEvidence("copper-top", "Top copper")}; ${roleEvidence("copper-bottom", "bottom copper")}. Confirm a deliberate single-layer design or supply a drawable layer.`,
  ));

  const topMask = renderedRoles("mask-top");
  const bottomMask = renderedRoles("mask-bottom");
  checks.push(check(
    "solder-mask",
    "Solder-mask pair",
    topMask.length === 1 && bottomMask.length === 1 ? "pass" : "warning",
    topMask.length === 1 && bottomMask.length === 1
      ? "Top and bottom solder-mask layers rendered."
      : `${roleEvidence("mask-top", "Top mask")}; ${roleEvidence("mask-bottom", "bottom mask")}. Confirm any intentionally maskless or valid-empty side.`,
  ));

  const assemblyCandidates = inventory.filter((item) => item.role.key.startsWith("paste-") || item.role.key.startsWith("silk-"));
  const assemblyLayers = assemblyCandidates.filter((item) => item.status === "rendered");
  const emptyAssemblyLayers = assemblyCandidates.filter((item) => item.status === "empty");
  const rejectedAssemblyLayers = assemblyCandidates.filter((item) => item.status === "rejected");
  checks.push(check(
    "assembly-layers",
    "Assembly layer presence",
    "info",
    assemblyLayers.length || emptyAssemblyLayers.length
      ? `${assemblyLayers.length} drawable and ${emptyAssemblyLayers.length} valid-empty paste/legend layer${assemblyCandidates.length === 1 ? " was" : "s were"} parsed${rejectedAssemblyLayers.length ? `; ${rejectedAssemblyLayers.length} additional candidate${rejectedAssemblyLayers.length === 1 ? " was" : "s were"} rejected` : ""}. These are inventoried, not checked against the BOM or placements.`
      : rejectedAssemblyLayers.length
        ? `${rejectedAssemblyLayers.length} paste/legend layer candidate${rejectedAssemblyLayers.length === 1 ? " was" : "s were"} identified but rejected by the parser.`
        : "No paste or legend layers recognized. These are optional for bare-board fabrication but normally needed for assembly outputs.",
  ));

  const unknown = inventoryRoles("unknown");
  const singletonRoles = ["copper-top", "copper-bottom", "mask-top", "mask-bottom", "paste-top", "paste-bottom", "silk-top", "silk-bottom", "outline"];
  const duplicateRoles = singletonRoles.filter((role) => inventoryRoles(role).length > 1);
  checks.push(check(
    "layer-identification",
    "Layer role identification",
    unknown.length || duplicateRoles.length ? "warning" : "pass",
    unknown.length || duplicateRoles.length
      ? `${unknown.length ? `${unknown.length} unclassified file${unknown.length === 1 ? "" : "s"}` : "No unclassified files"}${duplicateRoles.length ? `; duplicate candidates for ${duplicateRoles.map((role) => GERBER_ROLE_DEFINITIONS[role].label.toLowerCase()).join(", ")}` : ""}. Verify the inventory manually.`
      : "Every supplied Gerber was assigned one unambiguous manufacturing role.",
  ));

  const profileBounds = unionBounds(profiles.map((item) => item.bounds));
  const registrationLayers = drawable.filter((item) => !["outline", "documentation", "drill-drawing"].includes(item.role.key));
  const disjoint = profileBounds ? registrationLayers.filter((item) => item.bounds && !boundsOverlap(item.bounds, profileBounds)) : [];
  checks.push(check(
    "coarse-registration",
    "Coarse coordinate overlap",
    !profileBounds || !registrationLayers.length ? "not-reviewed" : disjoint.length ? "warning" : "pass",
    !profileBounds
      ? "Cannot compare layer extents without a rendered board profile."
      : !registrationLayers.length
        ? "No manufacturing layers were available to compare with the profile."
        : disjoint.length
          ? `${disjoint.length} layer${disjoint.length === 1 ? "" : "s"} do not overlap the profile extent: ${disjoint.map((item) => item.filename).join(", ")}.`
          : `All ${registrationLayers.length} drawable manufacturing layers overlap the profile extent. This is an extent check, not fiducial registration.`,
  ));

  checks.push(
    check("drill-data", "Drill and route validation", "not-reviewed", "Excellon tool tables, plated/non-plated holes, slots, and drill-to-copper registration are not parsed by this canvas."),
    check("manufacturing-drc", "Electrical / fabricator DRC", "not-reviewed", "Clearance, connectivity, netlist, annular ring, mask sliver, impedance, and fabricator capability rules are not inferred from the visual layers."),
  );

  return {
    inventory,
    checks,
    scope: {
      reviewed: "Layer role recognition, supported RS-274X draws/flashes, expected file presence, and coarse profile overlap.",
      notReviewed: "Electrical connectivity, design-rule clearance, Excellon drill/rout data, stack-up/impedance, and final CAM sign-off.",
    },
  };
}

export function combineGerbers(models, options = {}) {
  const layers = models.flatMap((model) => model.layers).sort((a, b) => (a.role?.order ?? 999) - (b.role?.order ?? 999));
  const warnings = models.flatMap((model) => model.warnings);
  const hasGeometry = layers.some((layer) => layer.strokes.length || layer.flashes.length || layer.labels.length || (layer.polygons || []).length);
  const combined = hasGeometry
    ? finishModel("Gerber RS-274X", layers, warnings)
    : emptyModel("Gerber RS-274X", layers, warnings);
  combined.inspection = inspectGerberSet(combined, options);
  return combined;
}

export function createBoardViewer(canvas) {
  const context = canvas.getContext("2d");
  const operationCache = new WeakMap();
  let layerCanvas = null;
  let model = null;
  let scale = 1;
  let offsetX = 0;
  let offsetY = 0;
  let dragging = null;
  let pixelRatio = 1;

  const sizeCanvas = () => {
    const rect = canvas.getBoundingClientRect();
    const requestedPixelRatio = Number(window.devicePixelRatio) || 1;
    pixelRatio = Math.max(1, Math.min(2, requestedPixelRatio));
    const width = Math.max(1, Math.round(rect.width * pixelRatio));
    const height = Math.max(1, Math.round(rect.height * pixelRatio));
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
    return rect;
  };
  const draw = () => {
    const rect = sizeCanvas();
    context.clearRect(0, 0, rect.width, rect.height);
    context.fillStyle = "#071b17";
    context.fillRect(0, 0, rect.width, rect.height);
    context.fillStyle = "rgba(218,254,69,.11)";
    for (let gx = 12; gx < rect.width; gx += 24) for (let gy = 12; gy < rect.height; gy += 24) context.fillRect(gx, gy, 1, 1);
    if (!model) return;
    const sx = (x) => x * scale + offsetX;
    const sy = (y) => y * scale + offsetY;
    for (const layer of model.layers.filter(({ visible }) => visible)) {
      if (!layerCanvas) {
        layerCanvas = document.createElement("canvas");
      }
      if (layerCanvas.width !== canvas.width) layerCanvas.width = canvas.width;
      if (layerCanvas.height !== canvas.height) layerCanvas.height = canvas.height;
      const layerContext = layerCanvas.getContext("2d");
      layerContext.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
      layerContext.clearRect(0, 0, rect.width, rect.height);
      layerContext.strokeStyle = layer.color;
      layerContext.fillStyle = layer.color;
      layerContext.globalAlpha = 1;
      layerContext.lineCap = "round";
      layerContext.lineJoin = "round";
      let operations = operationCache.get(layer);
      if (!operations) {
        let fallbackOrder = 1_000_000;
        operations = [
          ...layer.strokes.map((item) => ({ kind: "stroke", item, order: item.order ?? fallbackOrder++ })),
          ...layer.flashes.map((item) => ({ kind: "flash", item, order: item.order ?? fallbackOrder++ })),
          ...(layer.polygons || []).map((item) => ({ kind: "polygon", item, order: item.order ?? fallbackOrder++ })),
          ...layer.labels.map((item) => ({ kind: "label", item, order: item.order ?? fallbackOrder++ })),
        ].sort((left, right) => left.order - right.order);
        operationCache.set(layer, operations);
      }
      for (const { kind, item } of operations) {
        layerContext.globalCompositeOperation = item.polarity === "clear" ? "destination-out" : "source-over";
        if (kind === "stroke") {
          layerContext.lineWidth = Math.max(1, Math.min(22, item.width * scale));
          layerContext.beginPath();
          layerContext.moveTo(sx(item.x1), sy(item.y1));
          layerContext.lineTo(sx(item.x2), sy(item.y2));
          layerContext.stroke();
          continue;
        }
        if (kind === "flash") {
          layerContext.save();
          layerContext.translate(sx(item.x), sy(item.y));
          layerContext.rotate(item.rotation || 0);
          const width = Math.max(2, item.w * scale);
          const height = Math.max(2, item.h * scale);
          if (item.shape === "roundrect-macro") {
            const points = item.points.map((point) => ({ x: point.x * scale, y: point.y * scale }));
            layerContext.beginPath();
            layerContext.moveTo(points[0].x, points[0].y);
            for (const point of points.slice(1)) layerContext.lineTo(point.x, point.y);
            layerContext.closePath();
            layerContext.fill();
            layerContext.lineWidth = Math.max(1, item.radius * 2 * scale);
            layerContext.lineCap = "round";
            layerContext.lineJoin = "round";
            layerContext.stroke();
            for (const point of points) {
              layerContext.beginPath();
              layerContext.arc(point.x, point.y, Math.max(1, item.radius * scale), 0, Math.PI * 2);
              layerContext.fill();
            }
            layerContext.restore();
            continue;
          }
          layerContext.beginPath();
          if (item.shape === "circle") layerContext.arc(0, 0, width / 2, 0, Math.PI * 2);
          else if (item.shape === "oval") {
            const radius = Math.min(width, height) / 2;
            if (width >= height) {
              layerContext.moveTo(-width / 2 + radius, -height / 2);
              layerContext.lineTo(width / 2 - radius, -height / 2);
              layerContext.arc(width / 2 - radius, 0, radius, -Math.PI / 2, Math.PI / 2);
              layerContext.lineTo(-width / 2 + radius, height / 2);
              layerContext.arc(-width / 2 + radius, 0, radius, Math.PI / 2, -Math.PI / 2);
            } else {
              layerContext.moveTo(-width / 2, -height / 2 + radius);
              layerContext.arc(0, -height / 2 + radius, radius, Math.PI, 0);
              layerContext.lineTo(width / 2, height / 2 - radius);
              layerContext.arc(0, height / 2 - radius, radius, 0, Math.PI);
            }
            layerContext.closePath();
          }
          else layerContext.rect(-width / 2, -height / 2, width, height);
          layerContext.fill();
          if (item.drill) {
            layerContext.globalCompositeOperation = "destination-out";
            layerContext.beginPath();
            layerContext.arc(0, 0, Math.max(1.5, item.drill * scale / 2), 0, Math.PI * 2);
            layerContext.fill();
          }
          layerContext.restore();
          continue;
        }
        if (kind === "polygon") {
          layerContext.beginPath();
          for (const contour of item.contours) {
            if (!contour.length) continue;
            layerContext.moveTo(sx(contour[0].x), sy(contour[0].y));
            for (const point of contour.slice(1)) layerContext.lineTo(sx(point.x), sy(point.y));
            layerContext.closePath();
          }
          layerContext.fill("evenodd");
          continue;
        }
        layerContext.font = "600 10px ui-monospace, SFMono-Regular, Menlo, monospace";
        if (item.text) layerContext.fillText(item.text, sx(item.x) + 5, sy(item.y) - 5);
      }
      layerContext.globalCompositeOperation = "source-over";
      context.globalCompositeOperation = "source-over";
      context.globalAlpha = layer.id.startsWith("mask") ? 0.46 : 0.9;
      context.drawImage(layerCanvas, 0, 0, rect.width, rect.height);
    }
    context.globalCompositeOperation = "source-over";
    context.globalAlpha = 1;
  };
  const fit = () => {
    if (!model) return draw();
    const rect = canvas.getBoundingClientRect();
    if (rect.width < 80 || rect.height < 80) return draw();
    scale = Math.min((rect.width - 70) / model.bounds.width, (rect.height - 70) / model.bounds.height);
    offsetX = (rect.width - model.bounds.width * scale) / 2 - model.bounds.minX * scale;
    offsetY = (rect.height - model.bounds.height * scale) / 2 - model.bounds.minY * scale;
    draw();
  };
  const zoom = (factor, x = canvas.clientWidth / 2, y = canvas.clientHeight / 2) => {
    const next = Math.max(0.05, Math.min(500, scale * factor));
    offsetX = x - ((x - offsetX) / scale) * next;
    offsetY = y - ((y - offsetY) / scale) * next;
    scale = next;
    draw();
  };
  canvas.addEventListener("wheel", (event) => {
    event.preventDefault();
    const rect = canvas.getBoundingClientRect();
    zoom(event.deltaY < 0 ? 1.15 : 1 / 1.15, event.clientX - rect.left, event.clientY - rect.top);
  }, { passive: false });
  canvas.addEventListener("pointerdown", (event) => {
    dragging = { x: event.clientX, y: event.clientY, offsetX, offsetY };
    canvas.setPointerCapture(event.pointerId);
  });
  canvas.addEventListener("pointermove", (event) => {
    if (!dragging) return;
    offsetX = dragging.offsetX + event.clientX - dragging.x;
    offsetY = dragging.offsetY + event.clientY - dragging.y;
    draw();
  });
  canvas.addEventListener("pointerup", () => (dragging = null));
  new ResizeObserver(fit).observe(canvas);
  return {
    setModel(next) { model = next; canvas.setAttribute("aria-label", next ? `${next.format} viewer with ${next.primitives} visible primitives` : "PCB viewer has no local geometry"); fit(); },
    setLayerVisible(id, visible) { const layer = model?.layers.find((item) => item.id === id); if (layer) layer.visible = visible; draw(); },
    fit,
    zoom,
  };
}
