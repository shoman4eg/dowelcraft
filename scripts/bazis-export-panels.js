// BAZIS-Script: exports the current model's panels to JSON — a reference for checking dowel-craft.
// Run in Bazis-Mebelshchik: Service → Scripts. The file is written next to the model or to the current directory.
// shortcut: API names come from the BAZIS-Script docs, unverified on a live Bazis;
// if a property is not found, it goes into the errors field and the script does not fail.
const fs = require('fs');

function safe(f, errors, name) {
  try { return f(); } catch (e) { errors.push(name + ': ' + e); return null; }
}

function vec(v) { return v ? { x: v.x, y: v.y, z: v.z } : null; }

function contour(c) {
  const out = [];
  for (let i = 0; i < c.Count; i++) {
    const e = c.Objects[i];
    out.push({ type: String(e.ElType), p1: vec(e.Pos1), p2: vec(e.Pos2), center: vec(e.Center) });
  }
  return out;
}

function butts(p) {
  const out = [];
  for (let i = 0; i < p.Butts.Count; i++) {
    const b = p.Butts[i];
    out.push({
      material: b.Material, sign: b.Sign, thickness: b.Thickness, width: b.Width,
      elem: b.ElemIndex, clip: b.ClipPanel, allowance: b.Allowance, overhung: b.Overhung,
    });
  }
  return out;
}

const panels = [];
function walk(list) {
  for (let i = 0; i < list.Count; i++) {
    const o = list.Objects[i];
    if (o.List) walk(o.List);
    if (o.Contour === undefined || o.Thickness === undefined) continue;
    const errors = [];
    panels.push({
      name: o.Name,
      artPos: o.ArtPos,
      designation: o.Designation,
      material: safe(() => o.MaterialName, errors, 'MaterialName'),
      thickness: o.Thickness,
      textureOrientation: safe(() => String(o.TextureOrientation), errors, 'TextureOrientation'),
      gsize: safe(() => vec(o.GSize), errors, 'GSize'),
      useInCutting: safe(() => o.UseInCutting, errors, 'UseInCutting'),
      contour: safe(() => contour(o.Contour), errors, 'Contour'),
      butts: safe(() => butts(o), errors, 'Butts'),
      errors,
    });
  }
}

walk(typeof Model !== 'undefined' ? Model : currentFileData.model);
const enums = safe(() => JSON.stringify(panelOperations.textureOrientation), [], 'enum');
const file = 'dowel-craft-export.json';
fs.writeFileSync(file, JSON.stringify({ textureOrientationEnum: enums, panels }, null, 2));
(typeof alert === 'function' ? alert : console.log)('Выгружено панелей: ' + panels.length + ' → ' + file);
