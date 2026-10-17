// Объекты BAZIS-Script поверх моста __h(команда, JSON) → JSON. Имена и поведение — по справке
// BAZIS-Script и по скриптам пользователей (reference/scripts). Чего нет в программе — бросает ошибку
// с объяснением, а не делает вид, что сработало.
(function (G) {
'use strict';
var __call = function (cmd, args) {
  var r = __h(cmd, JSON.stringify(args === undefined ? null : args));
  return r === undefined ? null : JSON.parse(r);
};
var __exec = function (cmd, args) { return __call('exec', { cmd: cmd, args: args }); };
var __finishers = [];
var __finish = function () { var f = __finishers; __finishers = []; f.forEach(function (fn) { fn(); }); };
var __text = function (args) { return Array.prototype.map.call(args, function (a) { return typeof a === 'object' ? JSON.stringify(a) : String(a); }).join(' '); };
var __unsupported = function (what) { return function () { throw new Error(what + ': недоступно в basis-craft'); }; };

// ── Векторы и точки ──
function NewVector(x, y, z) { return { x: x || 0, y: y || 0, z: z || 0 }; }
function VectorMake(x, y, z) { return NewVector(x, y, z); }
function NewPoint(x, y) { return { x: x || 0, y: y || 0 }; }
// NewNumber/NewString: с окном-владельцем — поле ввода, иначе — значение.
function NewNumber(v, parent) { if (v && v.Children) return __control('Number', v, parent); return { Value: v || 0 }; }
function NewString(v, parent) { if (v && v.Children) return __control('TextEdit', v, parent); return { Value: v || '' }; }

// ── Контур (T2DElemList) ──
function T2DElement() {}
function T2DLine() {} T2DLine.prototype = Object.create(T2DElement.prototype);
function T2DArc() {} T2DArc.prototype = Object.create(T2DElement.prototype);
function T2DCircle() {} T2DCircle.prototype = Object.create(T2DElement.prototype);
function T2DElemList() {}
function __elem(s) {
  var e;
  if (s.Line) e = { ElType: 1, Pos1: NewPoint(s.Line.a[0], s.Line.a[1]), Pos2: NewPoint(s.Line.b[0], s.Line.b[1]) };
  else if (s.Arc) {
    var a = s.Arc, r = Math.hypot(a.a[0] - a.c[0], a.a[1] - a.c[1]);
    e = { ElType: 2, Pos1: NewPoint(a.a[0], a.a[1]), Pos2: NewPoint(a.b[0], a.b[1]), Center: NewPoint(a.c[0], a.c[1]), ArcDir: !!a.ccw, ArcRadius: r };
  } else {
    var c = s.Circle;
    e = { ElType: 3, Center: NewPoint(c.c[0], c.c[1]), CirRadius: c.r, Pos1: NewPoint(c.c[0] + c.r, c.c[1]), Pos2: NewPoint(c.c[0] + c.r, c.c[1]) };
  }
  e.IsLine = function () { return this.ElType === 1; };
  e.IsArc = function () { return this.ElType === 2; };
  e.IsCircle = function () { return this.ElType === 3; };
  e.ObjLength = function () {
    if (this.ElType === 1) return Math.hypot(this.Pos2.x - this.Pos1.x, this.Pos2.y - this.Pos1.y);
    if (this.ElType === 3) return 2 * Math.PI * this.CirRadius;
    var c = this.Center, a0 = Math.atan2(this.Pos1.y - c.y, this.Pos1.x - c.x), a1 = Math.atan2(this.Pos2.y - c.y, this.Pos2.x - c.x);
    var sw = this.ArcDir ? a1 - a0 : a0 - a1; sw = ((sw % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI) || 2 * Math.PI;
    return this.ArcRadius * sw;
  };
  e.Length = e.ObjLength;
  Object.setPrototypeOf(e, (e.ElType === 1 ? T2DLine : e.ElType === 2 ? T2DArc : T2DCircle).prototype);
  e.Data = {};
  e.LineCenter = function () { return NewPoint((this.Pos1.x + this.Pos2.x) / 2, (this.Pos1.y + this.Pos2.y) / 2); };
  e.IsList = function () { return false; };
  e.Gabarits = function () {
    if (this.ElType === 3) return { Min: NewPoint(this.Center.x - this.CirRadius, this.Center.y - this.CirRadius), Max: NewPoint(this.Center.x + this.CirRadius, this.Center.y + this.CirRadius) };
    var c = NewContour(); c.Objects.push(this); return c.Gabarits();
  };
  e.Size = function () { var g = this.Gabarits(); return NewPoint(g.Max.x - g.Min.x, g.Max.y - g.Min.y); };
  e.Move = function (dx, dy) {
    if (typeof dx === 'object') { dy = dx.y; dx = dx.x; }
    [this.Pos1, this.Pos2, this.Center].forEach(function (p) { if (p) { p.x += dx; p.y += dy; } });
  };
  e.InvertDirection = function () { var t = this.Pos1; this.Pos1 = this.Pos2; this.Pos2 = t; if (this.ElType === 2) this.ArcDir = !this.ArcDir; };
  e.__pts = function () { var c = NewContour(); c.Objects.push(this); return c.__poly(false); };
  e.ClosestPoint = function (p) {
    var pts = this.__pts(), best = null, bd = Infinity;
    for (var i = 0; i + 1 < pts.length; i++) {
      var a = pts[i], b = pts[i + 1], dx = b.x - a.x, dy = b.y - a.y, l = dx * dx + dy * dy || 1e-12;
      var t = Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l)), q = NewPoint(a.x + dx * t, a.y + dy * t), d = Math.hypot(q.x - p.x, q.y - p.y);
      if (d < bd) { bd = d; best = q; }
    }
    return best || NewPoint(this.Pos1.x, this.Pos1.y);
  };
  e.DistanceToPoint = function (p) { var q = this.ClosestPoint(p); return Math.hypot(q.x - p.x, q.y - p.y); };
  return e;
}
function __seg(e) {
  if (e.ElType === 1) return { Line: { a: [e.Pos1.x, e.Pos1.y], b: [e.Pos2.x, e.Pos2.y] } };
  if (e.ElType === 2) return { Arc: { a: [e.Pos1.x, e.Pos1.y], b: [e.Pos2.x, e.Pos2.y], c: [e.Center.x, e.Center.y], ccw: !!e.ArcDir } };
  return { Circle: { c: [e.Center.x, e.Center.y], r: e.CirRadius } };
}
function NewContour(segs, edges) {
  var c = Object.create(T2DElemList.prototype);
  c.Objects = (segs || []).map(__elem);
  // Кромка элемента — в Data.Butt, как у контура панели в Базисе.
  (edges || []).forEach(function (e) { var el = c.Objects[e.segment]; if (el) el.Data.Butt = { Material: e.material, Sign: __matName(e.material), Thickness: e.thickness, ClipPanel: e.clip }; });
  Object.defineProperty(c, 'Count', { get: function () { return this.Objects.length; } });
  c.Clear = function () { this.Objects = []; };
  c.AddLine = function (x1, y1, x2, y2) {
    if (typeof x1 === 'object') { y2 = y1.y; x2 = y1.x; y1 = x1.y; x1 = x1.x; }
    var e = __elem({ Line: { a: [x1, y1], b: [x2, y2] } }); this.Objects.push(e); return e;
  };
  c.AddArc = function (p1, p2, center, ccw) {
    var e = __elem({ Arc: { a: [p1.x, p1.y], b: [p2.x, p2.y], c: [center.x, center.y], ccw: ccw === undefined ? true : !!ccw } }); this.Objects.push(e); return e;
  };
  c.AddCircle = function (x, y, r) {
    if (typeof x === 'object') { r = y; y = x.y; x = x.x; }
    var e = __elem({ Circle: { c: [x, y], r: r } }); this.Objects.push(e); return e;
  };
  c.AddRectangle = function (x1, y1, x2, y2) {
    if (typeof x1 === 'object') { y2 = y1.y; x2 = y1.x; y1 = x1.y; x1 = x1.x; }
    this.AddLine(x1, y1, x2, y1); this.AddLine(x2, y1, x2, y2); this.AddLine(x2, y2, x1, y2); this.AddLine(x1, y2, x1, y1);
  };
  c.Append = function (other) { var self = this; other.Objects.forEach(function (e) { self.Objects.push(e); }); };
  c.Gabarits = function () {
    var lo = NewPoint(Infinity, Infinity), hi = NewPoint(-Infinity, -Infinity);
    this.Objects.forEach(function (e) {
      [e.Pos1, e.Pos2].forEach(function (p) { lo.x = Math.min(lo.x, p.x); lo.y = Math.min(lo.y, p.y); hi.x = Math.max(hi.x, p.x); hi.y = Math.max(hi.y, p.y); });
      if (e.ElType === 3) { lo.x = Math.min(lo.x, e.Center.x - e.CirRadius); lo.y = Math.min(lo.y, e.Center.y - e.CirRadius); hi.x = Math.max(hi.x, e.Center.x + e.CirRadius); hi.y = Math.max(hi.y, e.Center.y + e.CirRadius); }
    });
    return { Min: lo, Max: hi };
  };
  Object.defineProperty(c, 'Min', { get: function () { return this.Gabarits().Min; } });
  Object.defineProperty(c, 'Max', { get: function () { return this.Gabarits().Max; } });
  Object.defineProperty(c, 'Width', { get: function () { var g = this.Gabarits(); return g.Max.x - g.Min.x; } });
  Object.defineProperty(c, 'Height', { get: function () { var g = this.Gabarits(); return g.Max.y - g.Min.y; } });
  c.ObjLength = function () { return this.Objects.reduce(function (s, e) { return s + e.ObjLength(); }, 0); };
  c.IsClosed = function () { return true; };
  c.IsClosedContour = function () { return true; };
  c.forEach = function (fn) { this.Objects.forEach(fn); };
  // Точки ломаной (дуги — по 32 хорды) в порядке элементов; closed — вернуть к первой.
  c.__poly = function (closed) {
    var out = [];
    this.Objects.forEach(function (e) {
      if (e.ElType === 1) { out.push(NewPoint(e.Pos1.x, e.Pos1.y)); out.push(NewPoint(e.Pos2.x, e.Pos2.y)); return; }
      var ctr = e.Center, r = e.ElType === 3 ? e.CirRadius : e.ArcRadius;
      var a0 = e.ElType === 3 ? 0 : Math.atan2(e.Pos1.y - ctr.y, e.Pos1.x - ctr.x), sw = 2 * Math.PI;
      if (e.ElType === 2) { var a1 = Math.atan2(e.Pos2.y - ctr.y, e.Pos2.x - ctr.x); sw = e.ArcDir ? a1 - a0 : a0 - a1; sw = ((sw % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI) || 2 * Math.PI; }
      var sg = e.ElType === 2 && !e.ArcDir ? -1 : 1;
      for (var i = 0; i <= 32; i++) { var t = a0 + sg * sw * i / 32; out.push(NewPoint(ctr.x + r * Math.cos(t), ctr.y + r * Math.sin(t))); }
    });
    if (closed && out.length) out.push(out[0]);
    return out;
  };
  c.IsContourRectangle = function () {
    if (this.Objects.length !== 4 || this.Objects.some(function (e) { return e.ElType !== 1; })) return false;
    return this.Objects.every(function (e) { return Math.abs(e.Pos1.x - e.Pos2.x) < 1e-6 || Math.abs(e.Pos1.y - e.Pos2.y) < 1e-6; });
  };
  c.IsCounterClockWise = function () {
    var p = this.__poly(false), a = 0;
    for (var i = 0; i < p.length; i++) { var q = p[(i + 1) % p.length]; a += p[i].x * q.y - q.x * p[i].y; }
    return a > 0;
  };
  c.IsPointInside = function (pt) {
    var p = this.__poly(true), inside = false;
    for (var i = 0; i + 1 < p.length; i++) {
      var a = p[i], b = p[i + 1];
      if ((a.y > pt.y) !== (b.y > pt.y) && pt.x < a.x + (pt.y - a.y) / (b.y - a.y) * (b.x - a.x)) inside = !inside;
    }
    return inside;
  };
  c.IsInContour = function (other) { var self = this; return self.__poly(false).every(function (pt) { return other.IsPointInside(pt); }); };
  c.IndexOf = function (e) { return this.Objects.indexOf(e); };
  c.Delete = function (i) { this.Objects.splice(typeof i === 'number' ? i : this.Objects.indexOf(i), 1); };
  c.Exchange = function (i, j) { var t = this.Objects[i]; this.Objects[i] = this.Objects[j]; this.Objects[j] = t; };
  c.Add = function (e) { this.Objects.push(e); return e; };
  c.AddCopy = function (e) { var x = __elem(__seg(e)); this.Objects.push(x); return x; };
  c.AddList = function (l) { this.Append(l); };
  c.LineTo = function (x, y) { var last = this.Objects[this.Objects.length - 1]; return this.AddLine(last.Pos2.x, last.Pos2.y, x, y); };
  c.AddArc3 = function (p1, p2, p3) {
    // Окружность через три точки: центр — пересечение серединных перпендикуляров.
    var ax = p1.x, ay = p1.y, bx = p2.x, by = p2.y, cx = p3.x, cy = p3.y, d = 2 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if (Math.abs(d) < 1e-12) return this.AddLine(ax, ay, cx, cy);
    var ux = ((ax * ax + ay * ay) * (by - cy) + (bx * bx + by * by) * (cy - ay) + (cx * cx + cy * cy) * (ay - by)) / d;
    var uy = ((ax * ax + ay * ay) * (cx - bx) + (bx * bx + by * by) * (ax - cx) + (cx * cx + cy * cy) * (bx - ax)) / d;
    var ccw = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax) > 0;
    return this.AddArc(p1, p3, NewPoint(ux, uy), ccw);
  };
  c.FindNearestObject = function (p) { var best = null, bd = Infinity; this.Objects.forEach(function (e) { var d = e.DistanceToPoint(p); if (d < bd) { bd = d; best = e; } }); return best; };
  c.OrderContours = function () {};
  // Эквидистанта: только для прямоугольного контура (габарит ± смещение).
  c.AddEquidistant = function (src, offset) {
    if (!src.IsContourRectangle()) throw new Error('AddEquidistant: для фигурного контура недоступно');
    var g = src.Gabarits();
    this.AddRectangle(g.Min.x - offset, g.Min.y - offset, g.Max.x + offset, g.Max.y + offset);
  };
  // Доступ к элементам по номеру: contour[i] — как в Базисе.
  return new Proxy(c, { get: function (t, k) { if (typeof k === 'string' && /^\d+$/.test(k)) return t.Objects[+k]; return t[k]; } });
}

// ── Детали модели ──
var __matName = function (full) { return String(full).split('\r')[0]; };
function __panel(key) {
  var p = { __key: key, __rev: -1, __d: null, List: false };
  var d = function () {
    var rev = __call('revision');
    if (p.__rev !== rev) { p.__d = __call('part', key); p.__rev = rev; }
    return p.__d;
  };
  var set = function (field, value) {
    var x = d();
    if (!x.id) throw new Error('«' + x.name + '» — деталь шаблона (корпуса, модуля): править отдельно можно после «Разрушить»');
    var a = { id: x.id }; a[field] = value;
    __exec('panel.set', a);
  };
  var prop = function (name, get, put) { Object.defineProperty(p, name, { get: get, set: put || function () { throw new Error(name + ': только чтение'); }, enumerable: true }); };
  prop('Name', function () { return d().name; }, function (v) { set('name', String(v)); });
  prop('ArtPos', function () { return d().pos; }, function (v) { set('pos', String(v)); });
  prop('Designation', function () { return d().designation; }, function (v) { set('designation', String(v)); });
  prop('MaterialName', function () { return __matName(d().material); });
  prop('Material', function () { return { Name: __matName(d().material), MaterialName: d().material }; });
  prop('Thickness', function () { return d().thickness; });
  prop('GSize', function () { var x = d(); return NewVector(x.size[0], x.size[1], x.thickness); });
  prop('GMin', function () { return NewVector(0, 0, 0); });
  prop('GMax', function () { var x = d(); return NewVector(x.size[0], x.size[1], x.thickness); });
  prop('GabMin', function () { var o = d().origin; return NewVector(o[0], o[1], o[2]); });
  prop('GabMax', function () { var x = d(); return NewVector(x.origin[0] + x.box[0], x.origin[1] + x.box[1], x.origin[2] + x.box[2]); });
  prop('Position', function () { var f = d().frame; return NewVector(f.origin[0], f.origin[1], f.origin[2]); });
  prop('ContourWidth', function () { return d().size[0]; });
  prop('ContourHeight', function () { return d().size[1]; });
  prop('TextureOrientation', function () { var t = d().texture; return t === 'Horizontal' ? 1 : t === 'Vertical' ? 2 : 0; },
    function (v) { set('texture', v === 1 ? 'Horizontal' : v === 2 ? 'Vertical' : 'Default'); });
  prop('Contour', function () { return NewContour(d().segs, d().edges); });
  prop('Butts', function () {
    var b = d().edges.map(function (e) { return { Material: e.material, Sign: __matName(e.material), Thickness: e.thickness, Width: e.width, ElemIndex: e.segment, ClipPanel: e.clip, Allowance: e.allowance, Overhung: 0 }; });
    b.Count = b.length; return b;
  });
  prop('Holes', function () { return d().holes; });
  prop('Kind', function () { return d().kind; });
  p.Selected = false; p.Visible = true; p.UserProperty = {};
  p.Build = function () {
    if (p.__contour) { var x = d(); if (!x.id) throw new Error('контур детали шаблона правят после «Разрушить»'); __exec('panel.contour', { id: x.id, segs: p.__contour.Objects.map(__seg) }); p.__contour = null; }
  };
  p.SetContour = function (c) { p.__contour = c; };
  var frame = function () { return d().frame; };
  p.ToGlobal = function (v) { var f = frame(); return NewVector(f.origin[0] + f.x[0] * v.x + f.y[0] * v.y + f.z[0] * v.z, f.origin[1] + f.x[1] * v.x + f.y[1] * v.y + f.z[1] * v.z, f.origin[2] + f.x[2] * v.x + f.y[2] * v.y + f.z[2] * v.z); };
  p.NToGlobal = function (v) { var f = frame(); return NewVector(f.x[0] * v.x + f.y[0] * v.y + f.z[0] * v.z, f.x[1] * v.x + f.y[1] * v.y + f.z[1] * v.z, f.x[2] * v.x + f.y[2] * v.y + f.z[2] * v.z); };
  p.ToObject = function (v) { var f = frame(), dx = v.x - f.origin[0], dy = v.y - f.origin[1], dz = v.z - f.origin[2]; return NewVector(dx * f.x[0] + dy * f.x[1] + dz * f.x[2], dx * f.y[0] + dy * f.y[1] + dz * f.y[2], dx * f.z[0] + dy * f.z[1] + dz * f.z[2]); };
  p.NToObject = function (v) { var f = frame(); return NewVector(v.x * f.x[0] + v.y * f.x[1] + v.z * f.x[2], v.x * f.y[0] + v.y * f.y[1] + v.z * f.y[2], v.x * f.z[0] + v.y * f.z[1] + v.z * f.z[2]); };
  p.Delete = function () { var x = d(); __exec('object.remove', { ids: [x.id || x.owner] }); };
  p.AddButt = function (material, elem) { var x = d(); __exec('panel.edge', { id: x.id, segments: [elem], edge: material }); };
  var empty = function () { return { Count: 0, Objects: [] }; };
  prop('Plastics', empty); prop('Cuts', empty); prop('Bent', function () { return false; });
  prop('FrontFace', function () { return 0; }); prop('InterlayerThickness', function () { return 0; });
  prop('LowZ', function () { return 0; }); prop('HighZ', function () { return d().thickness; });
  prop('MiddleZ', function () { return d().thickness / 2; }); prop('ZThickness', function () { return d().thickness; });
  prop('GCenter', function () { var x = d(); return NewVector(x.size[0] / 2, x.size[1] / 2, x.thickness / 2); });
  prop('PositionX', function () { return d().frame.origin[0]; }); prop('PositionY', function () { return d().frame.origin[1]; }); prop('PositionZ', function () { return d().frame.origin[2]; });
  prop('UID', function () { return d().key.join('.'); });
  prop('Owner', function () { return Model; });
  prop('Model', function () { return Model; });
  ['UseInCNC', 'UseInCutting', 'UseInDocs', 'UseInEstimate', 'UseInInspection'].forEach(function (n) { prop(n, function () { return true; }); });
  ['Picked', 'Highlighted', 'IsAssemblyUnit', 'MakeAssemblyDrawing', 'MakeExplodedView'].forEach(function (n) { prop(n, function () { return false; }); });
  p.Notes = ''; p.Color = 0; p.UserPropCount = 0;
  p.AsList = function () { return null; };
  p.IsMesh = function () { return false; };
  p.IsVisible = function () { return true; };
  p.IsHighlighted = function () { return false; };
  p.IsSelected = function () { var k = JSON.stringify(key); return __call('selection').some(function (s) { return JSON.stringify(s) === k; }); };
  p.HasUserProperty = function (n) { return Object.prototype.hasOwnProperty.call(p.UserProperty, n); };
  p.ObjectToGlobal = p.ToGlobal; p.GlobalToObject = p.ToObject; p.NObjectToGlobal = p.NToGlobal; p.NGlobalToObject = p.NToObject;
  var ids = function () { var x = d(); return [x.id || x.owner]; };
  p.TranslateGCS = function (v) { __exec('object.move', { ids: ids(), d: [v.x || 0, v.y || 0, v.z || 0] }); };
  p.Translate = function (v) { var g = p.NToGlobal(v); p.TranslateGCS(g); };
  p.TranslateLCS = p.Translate; p.TranslateInCurSys = p.TranslateGCS;
  p.RotateGCS = function (axis, angle) {
    var a = typeof axis === 'object' ? (axis.x ? 'x' : axis.y ? 'y' : 'z') : String(axis).toLowerCase();
    var o = d().frame.origin;
    __exec('object.rotate', { ids: ids(), axis: a, angle: angle, center: o });
  };
  // Поворот вокруг своей оси: только когда она совпадает с осью проекта (иначе — точности ради — ошибка).
  var local = function (k) {
    return function (angle) {
      var f = d().frame, ax = [f.x, f.y, f.z][k], i = ax.findIndex(function (v) { return Math.abs(Math.abs(v) - 1) < 1e-9; });
      if (i < 0) throw new Error('поворот вокруг наклонной оси детали недоступен');
      __exec('object.rotate', { ids: ids(), axis: 'xyz'[i], angle: angle * ax[i], center: f.origin });
    };
  };
  p.RotateX = local(0); p.RotateY = local(1); p.RotateZ = local(2);
  p.Clone = function () { var r = __exec('object.copy', { ids: ids(), d: [0, 0, 0], count: 1 }); return __panel(__call('parts').filter(function (k) { return k[1] === r.ids[0] || k[0] === r.ids[0]; })[0]); };
  return p;
}
function ObjectIsPanel(o) { return !!(o && o.__key); }
function ObjectIsBlock(o) { return !!(o && o.List); }
['ObjectIsFastener', 'ObjectIsExtrusionBody', 'ObjectIsTrajectoryBody', 'ObjectIsRotationBody', 'ObjectIsBent', 'ObjectIsProfile'].forEach(function (n) { G[n] = function () { return false; }; });

// ── Модель ──
var Model = (function () {
  var m = { Name: '', Designation: '', UserProperty: {}, DS: {}, Highlights: [], HighlightCount: 0 };
  var panels = function () { return __call('parts').map(__panel); };
  // Объекты верхнего уровня — блоки по владельцу (корпус, модуль, блок), как в дереве Базиса.
  var blocks = function () {
    var by = {}, order = [];
    panels().forEach(function (p) { var o = p.__key[0]; if (!by[o]) { by[o] = []; order.push(o); } by[o].push(p); });
    return order.map(function (o) {
      var list = by[o];
      if (list.length === 1 && list[0].__key[1] === o) return list[0];
      var b = { List: true, Name: 'Объект ' + o, Objects: list, Count: list.length };
      b.forEach = function (fn) { list.forEach(fn); };
      return b;
    });
  };
  Object.defineProperty(m, 'Objects', { get: blocks });
  Object.defineProperty(m, 'Count', { get: function () { return blocks().length; } });
  Object.defineProperty(m, 'Selections', { get: function () { return __call('selection').map(__panel); } });
  Object.defineProperty(m, 'SelectionCount', { get: function () { return __call('selection').length; } });
  Object.defineProperty(m, 'Selected', { get: function () { var s = __call('selection'); return s.length ? __panel(s[0]) : null; } });
  m.forEach = function (fn) { panels().forEach(fn); };
  m.UnPickAll = function () { __call('unselect'); };
  Object.defineProperty(m, 'SelectedObj', { get: function () { return __call('selection').map(__panel); } });
  m.forEachPanel = function (fn) { panels().forEach(fn); };
  m.UnSelectAll = function () { __call('unselect'); };
  m.UnHighlightAll = function () {};
  m.Build = function () {}; m.Update = function () {}; m.UpdateScene = function () {};
  m.Clear = __unsupported('Model.Clear');
  return m;
})();
function UnSelectAll() { Model.UnSelectAll(); }
function DeleteObject(o) { o.Delete(); }
function ForEachObjectInList(list, fn) { (list.Objects || list).forEach(fn); }

// ── Создание панелей (старое API): активный материал — первый в проекте ──
var __activeMaterial = null, __activeEdge = null;
function __material() {
  if (__activeMaterial) return __activeMaterial;
  var p = __call('project');
  if (!p.materials.length) { __exec('material.add', { name: 'ЛДСП 16', thickness: 16 }); return 'ЛДСП 16'; }
  return p.materials[0].name;
}
function __newPanel(plane, a, b, pos) {
  var r = __exec('panel.add', { material: __material(), plane: plane, size: [a, b], position: pos });
  var keys = __call('parts').filter(function (k) { return k[1] === r.id; });
  return __panel(keys[0]);
}
function AddFrontPanel(x1, y1, x2, y2, z) { return __newPanel('front', Math.abs(x2 - x1), Math.abs(y2 - y1), [Math.min(x1, x2), Math.min(y1, y2), z]); }
function AddHorizPanel(x1, z1, x2, z2, y) { return __newPanel('horizontal', Math.abs(x2 - x1), Math.abs(z2 - z1), [Math.min(x1, x2), y, Math.min(z1, z2)]); }
function AddVertPanel(z1, y1, z2, y2, x) { return __newPanel('vertical', Math.abs(z2 - z1), Math.abs(y2 - y1), [x, Math.min(y1, y2), Math.min(z1, z2)]); }

// ── Модульное API (новые скрипты) ──
var objects3d = {
  PanelOrientation: { front: 'front', vertical: 'vertical', horizont: 'horizontal', horizontal: 'horizontal' },
  NewPanel: function (w, h, orientation) { return __newPanel(orientation || 'front', w, h, [0, 0, 0]); },
  DeleteObject: DeleteObject,
};
var panelOperations = {
  AddButt: function (panel, elem) { if (!__activeEdge) throw new Error('нет активной кромки: materialData.SetupActiveButtMaterial'); panel.AddButt(__activeEdge, elem); },
};
var materialData = {
  SetupActiveFurnMaterial: function (name, thickness) {
    var p = __call('project'), n = __matName(name);
    if (!p.materials.some(function (m) { return m.name === n; })) __exec('material.add', { name: n, thickness: thickness || 16 });
    __activeMaterial = n;
  },
  SetupActiveButtMaterial: function (name, thickness, width) {
    var p = __call('project'), n = __matName(name);
    if (!p.edges.some(function (e) { return e.name === n; })) __exec('edge.add', { name: n, thickness: thickness || 0.4, width: width || 0 });
    __activeEdge = n;
  },
  ChooseActiveFurnMaterial: function () {}, ChooseActiveButtMaterial: function () {},
};
var apiVersion = { version: 3, AwareAndThrowIfApiVersionIsLowestThan: function () {} };
function AwareAndThrowIfApiVersionIsLowestThan() {}

// ── Изменения и выполнение ──
['RegisterObjectChanging', 'CommitCurrentChanges', 'RevertCurrentChanges', 'RecursiveChanging', 'StartEditing', 'Changing', 'Repaint', 'ReTransform', 'FinishExecution', 'ContinueExecution', 'Finish', 'Continue', 'Commit', 'Update'].forEach(function (n) { G[n] = function () {}; });
var Action = {
  ModelFilename: '', DS: {}, Hint: '', Control: { EstimateData: '', Article: { Code: '', Name: '' } },
  Continue: function () {}, Finish: function () {}, Commit: function () {}, Cancel: function () {},
  BlinkHint: function () {}, ViewDir: function () {}, LoadModel: __unsupported('Action.LoadModel'),
  AsyncExec: function (fn) { fn(); },
  Properties: (function () {
    var props = { __items: [] };
    var item = function (kind) {
      return function (caption, value, extra) {
        var it = { __id: __controls.length, Kind: kind, Caption: caption, Value: value === undefined ? (kind === 'Number' ? 0 : kind === 'Bool' ? false : '') : value, ItemIndex: 0, Items: Array.isArray(extra) ? extra : [], Visible: true, Enabled: true };
        it.NewNumber = props.NewNumber; it.NewString = props.NewString; it.NewBool = props.NewBool; it.NewCombo = props.NewCombo; it.NewGroup = props.NewGroup; it.NewButton = props.NewButton;
        it.AddItem = function (s) { this.Items.push(s); };
        if (kind === 'Combo' && Array.isArray(value)) { it.Items = value; it.Value = value[0]; }
        props.__items.push(it); __controls.push(it); return it;
      };
    };
    ['Number', 'String', 'Bool', 'Combo', 'Group', 'Button', 'Material', 'Butt', 'Furniture', 'Color', 'Selector'].forEach(function (k) { props['New' + k] = item(k); });
    props.Load = function () {}; props.Save = function () {}; props.Clear = function () { props.__items = []; };
    return props;
  })(),
};
Object.defineProperty(Action, 'OnFinish', { set: function (fn) { if (typeof fn === 'function') __finishers.push(fn); } });

// ── Сообщения ──
function alert() { __call('log', __text(arguments)); }
function MessageBox() { __call('log', __text(arguments)); return 1; }
function ErrorBox() { __call('log', 'Ошибка: ' + __text(arguments)); }
function RunYesNoDialog() { __call('log', 'Вопрос (ответ «Да»): ' + __text(arguments)); return true; }
var console = { log: function () { __call('log', __text(arguments)); }, warn: function () { __call('log', __text(arguments)); }, error: function () { __call('log', __text(arguments)); } };

// ── Окна (старое API): описание запоминается; в окне программы показывается диалогом ──
// Живые окна: элементы с номерами, показанные формы; окно программы читает их через __forms(),
// а щелчки и ввод передаёт в __click / __change.
var __controls = [], __shown = [];
function __control(kind, owner, parent) {
  var c = { __id: __controls.length, Kind: kind, Caption: '', Text: '', Value: 0, Checked: false, ItemIndex: -1, Items: [], Visible: true, Enabled: true, Children: [], Left: 0, Top: 0, Width: 0, Height: 0 };
  c.SetLayout = function (l, t, w, h) { this.Left = l; this.Top = t; this.Width = w; this.Height = h; };
  c.AddItem = function (s) { this.Items.push(s); return this.Items.length - 1; };
  c.Clear = function () { this.Items = []; };
  c.SetText = function (s) { this.Text = s; };
  c.Add = function (s) { this.Items.push(s); };
  var host = parent || owner;
  if (host && host.Children) host.Children.push(c);
  __controls.push(c);
  return c;
}
var __plain = function (o) { return JSON.parse(JSON.stringify(o, function (k, v) { return typeof v === 'function' ? undefined : v; })); };
function __forms() {
  var props = Action.Properties.__items.length ? [{ Kind: 'Form', __id: -1, Caption: 'Параметры скрипта', Children: Action.Properties.__items }] : [];
  return __plain(__shown.filter(function (f) { return f.Visible; }).concat(props));
}
function __find(id) { return id >= 0 ? __controls[id] : null; }
function __click(id) {
  var c = __find(id);
  if (c && typeof c.OnClick === 'function') c.OnClick(c);
  if (c && typeof c.OnChange === 'function' && c.Kind === 'CheckBox') c.OnChange(c);
}
function __change(id, field, value) {
  var c = __find(id);
  if (!c) return;
  c[field] = value;
  if (field === 'ItemIndex' && c.Items) c.Text = c.Items[value];
  if (typeof c.OnChange === 'function') c.OnChange(c);
}
function NewForm() {
  var f = __control('Form'); f.Caption = 'Форма';
  f.Show = function () { f.Visible = true; if (__shown.indexOf(f) < 0) __shown.push(f); __call('form', __plain(f)); return 1; };
  f.ShowModal = f.Show;
  f.Close = function () { f.Visible = false; };
  return f;
}
['Button', 'Label', 'TextEdit', 'CheckBox', 'ComboBox', 'Combo', 'GroupBox', 'Number', 'ListBox', 'Group', 'Image', 'Edit', 'Memo', 'RadioButton', 'Panel', 'Splitter', 'Tab', 'TabSheet', 'ProgressBar', 'ColorBox'].forEach(function (k) {
  G['New' + k] = function (owner, parent) { return __control(k, owner, parent); };
});
var UI = { components: {} };


// ── Файлы (только папка скрипта) ──
var fs = {
  readFileSync: function (p) { return __call('fs.read', String(p)); },
  writeFileSync: function (p, data) { __call('fs.write', { path: String(p), data: String(data) }); },
  appendFileSync: function (p, data) { __call('fs.append', { path: String(p), data: String(data) }); },
  existsSync: function (p) { return __call('fs.exists', String(p)); },
  mkdirSync: function (p) { __call('fs.mkdir', String(p)); },
  unlinkSync: function (p) { __call('fs.remove', String(p)); },
  readdirSync: function (p) { return __call('fs.list', String(p)); },
  statSync: function (p) { var dir = __call('fs.isdir', String(p)); return { isDirectory: function () { return dir; }, isFile: function () { return !dir; } }; },
  renameSync: function (a, b) { fs.writeFileSync(b, fs.readFileSync(a)); fs.unlinkSync(a); },
};
var path = {
  join: function () { return Array.prototype.join.call(arguments, '/').replace(/\/+/g, '/'); },
  basename: function (p, ext) { var b = String(p).split(/[\\/]/).pop(); return ext && b.endsWith(ext) ? b.slice(0, -ext.length) : b; },
  dirname: function (p) { var s = String(p).split(/[\\/]/); s.pop(); return s.join('/'); },
  extname: function (p) { var m = /\.[^./\\]*$/.exec(String(p)); return m ? m[0] : ''; },
};
var os = { EOL: '\r\n', platform: function () { return 'linux'; }, homedir: function () { return '.'; }, tmpdir: function () { return '.'; }, userInfo: function () { return { username: '' }; } };
function require(name) {
  if (name === 'fs') return fs;
  if (name === 'path') return path;
  if (name === 'os') return os;
  throw new Error('require(«' + name + '»): модуль недоступен');
}
var system = {
  log: function () { __call('log', __text(arguments)); },
  include: function (name) { (0, eval)(__call('include', String(name))); },
  readTextFile: fs.readFileSync,
  writeTextFile: function (p, s) { fs.writeFileSync(p, s); },
  fileExists: fs.existsSync,
  getFileNameWithoutExtension: function (p) { return path.basename(p).replace(/\.[^.]*$/, ''); },
  askFileName: function () { return ''; }, askFolder: function () { return ''; }, askWriteTextFile: function () { return ''; },
  secureExec: __unsupported('system.secureExec'),
};
function NewCOMObject(name) { throw new Error('COM-объект «' + name + '» недоступен: скрипт рассчитан на Windows'); }
var COM = NewCOMObject;
var execution = { Finish: function () {}, Continue: function () {} };

// ── Отмена (Undo) и модульное API BAZIS-Script ──
var __noop = function () {};
// Модуль, которого нет в программе: любой вызов — понятная ошибка.
var __missing = function (module) {
  return new Proxy({}, { get: function (t, k) { if (k in t) return t[k]; if (typeof k !== 'string') return undefined; return function () { throw new Error(module + '.' + k + ': недоступно в basis-craft'); }; } });
};
// Объект с известными методами; неизвестные — ошибка с именем.
var __module = function (name, known) {
  return new Proxy(known, { get: function (t, k) { if (k in t) return t[k]; if (typeof k !== 'string') return undefined; return function () { throw new Error(name + '.' + k + ': недоступно в basis-craft'); }; } });
};
var Undo = __module('Undo', { Changing: __noop, RecursiveChanging: __noop, BeginOper: __noop, EndOper: __noop, Commit: __noop, Rollback: __noop, Last: null, OnClick: null, Width: 0, Top: 0, ShowHint: __noop, message: '' });
var currentFileData = { model: Model, filename: '', article: { Name: '' }, updateView: __noop };
Object.defineProperty(currentFileData.article, 'Name', { get: function () { return __call('project').name; } });
var geometry2d = __module('geometry2d', { VectorMake: function (x, y) { return NewPoint(x, y); }, NewTempContour: function () { return NewContour(); }, NewContour: function () { return NewContour(); } });
var __v3 = {
  VectorMake: NewVector,
  VectorAdd: function (a, b) { return NewVector(a.x + b.x, a.y + b.y, a.z + b.z); },
  VectorSub: function (a, b) { return NewVector(a.x - b.x, a.y - b.y, a.z - b.z); },
  VectorScale: function (a, k) { return NewVector(a.x * k, a.y * k, a.z * k); },
  VectorDot: function (a, b) { return a.x * b.x + a.y * b.y + a.z * b.z; },
  VectorCross: function (a, b) { return NewVector(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x); },
  VectorLength: function (a) { return Math.hypot(a.x, a.y, a.z); },
  VectorNormalize: function (a) { var l = Math.hypot(a.x, a.y, a.z) || 1; return NewVector(a.x / l, a.y / l, a.z / l); },
};
var geometry3d = __module('geometry3d', __v3);
var historyOperations = __module('historyOperations', { RegisterObjectChanging: __noop, CommitCurrentChanges: __noop, RevertCurrentChanges: __noop, RecursiveChanging: __noop, StartEditing: __noop });
execution = __module('execution', { FinishExecution: __noop, ContinueExecution: __noop, CancelExecution: __noop, Finish: __noop, Continue: __noop });
var interaction = __module('interaction', { events: {}, windowData: {}, getRequest: function () { return null; } });
var batchProcessing = __module('batchProcessing', { ForEachObjectInList: ForEachObjectInList, ForEachPanel: function (fn) { Model.forEachPanel(fn); } });
var objectTypeChecker = __module('objectTypeChecker', {
  ObjectIsPanel: ObjectIsPanel, ObjectIsBlock: ObjectIsBlock, ObjectIsStructObject: ObjectIsBlock,
  ObjectIsFastener: function () { return false; }, ObjectIsExtrusionBody: function () { return false; }, ObjectIsTrajectoryBody: function () { return false; },
  ObjectIsRotationBody: function () { return false; }, ObjectIsSize: function () { return false; }, ObjectIsFurnAsm: function () { return false; }, ObjectIsLimits: function () { return false; },
  ObjectTypeValue: function (o) { return ObjectIsPanel(o) ? 4002 : ObjectIsBlock(o) ? 1005 : 0; },
});
materialData = __module('materialData', Object.assign(materialData, {
  ExtractMaterialName: function (full) { return String(full).split('\r')[0]; },
  ExtractMaterialCode: function (full) { return String(full).split('\r')[1] || ''; },
}));
objects3d = __module('objects3d', Object.assign(objects3d, { NewContour: function () { return NewContour(); } }));
panelOperations = __module('panelOperations', Object.assign(panelOperations, { faceType: { front: 0, back: 1 } }));
var fastenerOperations = __module('fastenerOperations', { FindConnectedFasteners: function () { return []; } });
var elasticTransformation = __missing('elasticTransformation (эластичность)');
var objectTransformation = __missing('objectTransformation');
var arrangePositions = __missing('arrangePositions');
var modelIOOperations = __missing('modelIOOperations');
var objectData = __missing('objectData');
var Alert = alert;
var AxisX = NewVector(1, 0, 0), AxisY = NewVector(0, 1, 0), AxisZ = NewVector(0, 0, 1);
var ActiveMaterial = {};
Object.defineProperty(ActiveMaterial, 'Name', { get: function () { return __material(); } });
// Указание объекта мышью: без окна — первая выделенная деталь (или null).
function GetPanel(text) { __call('log', 'Указать панель: ' + (text || '')); return Model.Selected; }
function GetObject(text) { return GetPanel(text); }
function prompt(text, def) { __call('log', 'Запрос (ответ по умолчанию): ' + text); return def === undefined ? '' : def; }
function confirm(text) { __call('log', 'Подтверждение (ответ «Да»): ' + text); return true; }
function setTimeout(fn) { __finishers.push(fn); return 0; }
function setInterval() { throw new Error('setInterval: таймеры недоступны'); }

// Экспорт: глобальные имена — обычные (перезаписываемые) свойства, чтобы скрипт мог объявить свои fs, path и т.п.
G['__call'] = __call;
G['__finish'] = __finish;
G['NewVector'] = NewVector;
G['VectorMake'] = VectorMake;
G['NewPoint'] = NewPoint;
G['NewNumber'] = NewNumber;
G['NewString'] = NewString;
G['NewContour'] = NewContour;
G['__panel'] = __panel;
G['ObjectIsPanel'] = ObjectIsPanel;
G['ObjectIsBlock'] = ObjectIsBlock;
G['Model'] = Model;
G['UnSelectAll'] = UnSelectAll;
G['DeleteObject'] = DeleteObject;
G['ForEachObjectInList'] = ForEachObjectInList;
G['AddFrontPanel'] = AddFrontPanel;
G['AddHorizPanel'] = AddHorizPanel;
G['AddVertPanel'] = AddVertPanel;
G['objects3d'] = objects3d;
G['panelOperations'] = panelOperations;
G['materialData'] = materialData;
G['apiVersion'] = apiVersion;
G['AwareAndThrowIfApiVersionIsLowestThan'] = AwareAndThrowIfApiVersionIsLowestThan;
G['Action'] = Action;
G['alert'] = alert;
G['MessageBox'] = MessageBox;
G['ErrorBox'] = ErrorBox;
G['RunYesNoDialog'] = RunYesNoDialog;
G['console'] = console;
G['NewForm'] = NewForm;
G['UI'] = UI;
G['__forms'] = __forms; G['__click'] = __click; G['__change'] = __change;
G['fs'] = fs;
G['path'] = path;
G['require'] = require;
G['system'] = system;
G['NewCOMObject'] = NewCOMObject;
G['COM'] = COM;
G['execution'] = execution;
G['Undo'] = Undo;
G['currentFileData'] = currentFileData;
G['geometry2d'] = geometry2d;
G['geometry3d'] = geometry3d;
G['historyOperations'] = historyOperations;
G['interaction'] = interaction;
G['batchProcessing'] = batchProcessing;
G['objectTypeChecker'] = objectTypeChecker;
G['fastenerOperations'] = fastenerOperations;
G['elasticTransformation'] = elasticTransformation;
G['objectTransformation'] = objectTransformation;
G['arrangePositions'] = arrangePositions;
G['modelIOOperations'] = modelIOOperations;
G['objectData'] = objectData;
G['Alert'] = Alert;
G['AxisX'] = AxisX; G['AxisY'] = AxisY; G['AxisZ'] = AxisZ; G['ActiveMaterial'] = ActiveMaterial; G['GetPanel'] = GetPanel; G['GetObject'] = GetObject; G['os'] = os;
G['T2DElement'] = T2DElement; G['T2DLine'] = T2DLine; G['T2DArc'] = T2DArc; G['T2DCircle'] = T2DCircle; G['T2DElemList'] = T2DElemList;
G['prompt'] = prompt;
G['confirm'] = confirm;
G['setTimeout'] = setTimeout;
G['setInterval'] = setInterval;
// Окна нового API — те же, что и старые.
['NewForm'].concat(Object.keys(G).filter(function (k) { return /^New[A-Z]/.test(k); })).forEach(function (k) { if (typeof G[k] === 'function') UI.components[k] = G[k]; });
G['UI'] = UI;
G['__forms'] = __forms; G['__click'] = __click; G['__change'] = __change;
})(globalThis);
