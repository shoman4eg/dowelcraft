// BAZIS-Script: выгрузка панелей текущей модели в JSON — эталон для сверки basis-craft.
// Запуск в Базис-Мебельщике: Сервис → Скрипты. Файл пишется рядом с моделью или в текущий каталог.
// shortcut: имена API взяты из документации BAZIS-Script без проверки на живом Базисе;
// если свойство не найдено, оно попадает в поле errors, скрипт не падает.
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
const file = 'basis-craft-export.json';
fs.writeFileSync(file, JSON.stringify({ textureOrientationEnum: enums, panels }, null, 2));
(typeof alert === 'function' ? alert : console.log)('Выгружено панелей: ' + panels.length + ' → ' + file);
