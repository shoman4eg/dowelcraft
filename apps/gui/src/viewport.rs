//! Model window: 3D on the GPU (wgpu), like in Bazis — modes "No hidden lines", "Hidden thin",
//! "Wireframe", "Transparent"; front, left, top and axonometric views; LMB rotates, RMB pans,
//! the wheel zooms. Renders into its own texture (with antialiasing and depth) and is shown as an image.

use dowel::design::{Kind, Part};
use dowel::mesh::{self, Mesh};
use eframe::egui::{self, Color32, Vec2};
use eframe::egui_wgpu::{self, wgpu};
use wgpu::util::DeviceExt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// "No hidden lines": solids and visible edges.
    Solid,
    /// "Hidden thin": plus hidden edges as a light line.
    Hidden,
    /// "Wireframe": edges only.
    Wire,
    /// "Transparent" (x-ray): translucent solids, all edges, part axes.
    XRay,
    /// "Textured": materials as images (Baltlaminat and custom), with visible edges.
    Textured,
}

impl Mode {
    pub const ALL: [(Mode, &str); 5] = [(Mode::Solid, "Без невидимых"), (Mode::Hidden, "Невидимые тонкие"), (Mode::Wire, "Каркас"), (Mode::XRay, "Прозрачность"), (Mode::Textured, "В текстурах")];
}

/// Result of a click in the model window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Click {
    None,
    /// Click on a part (index in the part list).
    Part(usize),
    /// Click on empty space — clear the selection.
    Empty,
    /// Right button (no drag) — menu; the part under the cursor, if any.
    Menu(Option<usize>),
    /// Click on a fitting (object number); `true` — right button.
    Furniture(u32, bool),
    /// The selection was dragged with the mouse: offset in mm (along one axis, whole mm).
    Move([f64; 3]),
    /// A handle changed the size of the selected panel: new `lo`, `hi` in mm.
    Resize([f64; 3], [f64; 3]),
}

/// New bounds of panel `i` when handle `h` (−1 — edge `lo`, 1 — `hi`, 0 — leave) along axes
/// `axes` moved by `d` mm: edges snap to edges of other parts closer than `tol` mm, whole mm.
fn resized(parts: &[Part], i: usize, axes: [usize; 2], h: [i8; 2], d: [f64; 2], tol: f64) -> ([f64; 3], [f64; 3]) {
    let p = &parts[i];
    let (mut lo, mut hi) = (p.origin, std::array::from_fn(|k| p.origin[k] + p.size[k]));
    let snap = |k: usize, v: f64| -> f64 {
        let best = parts.iter().enumerate().filter(|(j, _)| *j != i).flat_map(|(_, q)| [q.origin[k], q.origin[k] + q.size[k]]).map(|e| e - v).filter(|e| e.abs() < tol).min_by(|a, b| a.abs().total_cmp(&b.abs()));
        (v + best.unwrap_or(0.0)).round()
    };
    for (n, &k) in axes.iter().enumerate() {
        match h[n] {
            -1 => lo[k] = snap(k, lo[k] + d[n]).min(hi[k] - 1.0),
            1 => hi[k] = snap(k, hi[k] + d[n]).max(lo[k] + 1.0),
            _ => {}
        }
    }
    (lo, hi)
}

/// A fitting in the window: position, bounds in its own axes, model mesh (empty — drawn as a box).
#[derive(Clone, Debug)]
pub struct FurnView {
    pub id: u32,
    pub frame: dowel::geom::Frame,
    pub lo: [f64; 3],
    pub hi: [f64; 3],
    pub selected: bool,
    pub mesh: std::sync::Arc<Vec<dowel::panel::Tri>>,
}

impl PartialEq for FurnView {
    /// Meshes are compared by pointer: one model — one loaded mesh.
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id && self.frame == o.frame && self.lo == o.lo && self.hi == o.hi && self.selected == o.selected && std::sync::Arc::ptr_eq(&self.mesh, &o.mesh)
    }
}

/// Camera: look-at point, angles and scale (mm per pixel), parallel projection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub target: [f32; 3],
    /// Rotation about the vertical, radians: 0 — looking from the front (along −Z).
    pub yaw: f32,
    /// Tilt: positive — looking from above.
    pub pitch: f32,
    pub mm_per_px: f32,
}

impl Camera {
    /// Default Bazis axonometry: 20° about OX and −20° about OY.
    pub const AXON: (f32, f32) = (-20.0, 20.0);

    /// "Camera to scene" direction, screen right and up.
    fn basis(&self) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let (sy, cy, sp, cp) = (self.yaw.sin(), self.yaw.cos(), self.pitch.sin(), self.pitch.cos());
        let forward = [sy * cp, -sp, -cy * cp];
        let right = [cy, 0.0, sy];
        let up = cross(right, forward);
        (forward, right, up)
    }

    /// "World → clip" matrix for a `w`×`h` pixel window; `depth` — half-length of the scene along the view.
    fn view_proj(&self, w: f32, h: f32, depth: f32) -> [[f32; 4]; 4] {
        let (f, r, u) = self.basis();
        let (sx, sy) = (2.0 / (w * self.mm_per_px), 2.0 / (h * self.mm_per_px));
        let t = self.target;
        let d = |a: [f32; 3]| -(a[0] * t[0] + a[1] * t[1] + a[2] * t[2]);
        // Depth 0..1: the near plane is `depth` in front of the look-at point.
        let sz = 0.5 / depth;
        // Matrix columns (WGSL is column-major).
        [
            [r[0] * sx, u[0] * sy, f[0] * sz, 0.0],
            [r[1] * sx, u[1] * sy, f[1] * sz, 0.0],
            [r[2] * sx, u[2] * sy, f[2] * sz, 0.0],
            [d(r) * sx, d(u) * sy, d(f) * sz + 0.5, 1.0],
        ]
    }

    /// View ray through a screen point offset by `off` pixels from the center: origin and direction.
    fn ray(&self, off: Vec2, depth: f32) -> ([f32; 3], [f32; 3]) {
        let (f, r, u) = self.basis();
        let o = std::array::from_fn(|k| self.target[k] + (off.x * r[k] - off.y * u[k]) * self.mm_per_px - f[k] * depth);
        (o, f)
    }

    /// Move the look-at point by `delta` screen pixels.
    fn pan(&mut self, delta: Vec2) {
        let (_, r, u) = self.basis();
        for k in 0..3 {
            self.target[k] += (-delta.x * r[k] + delta.y * u[k]) * self.mm_per_px;
        }
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Ray-triangle intersection (Möller–Trumbore): distance along the ray.
fn hit(o: [f32; 3], d: [f32; 3], t: &[[f32; 3]; 3]) -> Option<f32> {
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (e1, e2) = (sub(t[1], t[0]), sub(t[2], t[0]));
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-9 {
        return None;
    }
    let s = sub(o, t[0]);
    let u = dot(s, p) / det;
    let q = cross(s, e1);
    let v = dot(d, q) / det;
    let dist = dot(e2, q) / det;
    ((0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0 && dist > 0.0).then_some(dist)
}

/// How to color the scene (see `Scene::new`).
enum Look<'a> {
    Plain,
    Wire,
    Textured(&'a [(String, String, [f64; 2])]),
}

/// Color of the selected part.
const SELECTED: [f32; 3] = [1.0, 0.62, 0.2];

/// Part color by role: carcass — beige, back panel — gray, fronts — light blue.
fn color(kind: Kind) -> [f32; 3] {
    match kind {
        Kind::Body => [0.93, 0.86, 0.75],
        Kind::Back => [0.80, 0.80, 0.80],
        Kind::Door | Kind::Drawer => [0.68, 0.79, 0.90],
    }
}

const SAMPLES: u32 = 4;
const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Bytes per vertex: position, normal, RGBA color, texture coordinates.
const STRIDE: u64 = 48;

const SHADER: &str = r#"
struct U { view_proj: mat4x4<f32>, light: vec4<f32>, params: vec4<f32> };
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
struct VIn { @location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>, @location(3) uv: vec2<f32> };
struct VOut { @builtin(position) clip: vec4<f32>, @location(0) color: vec4<f32>, @location(1) uv: vec2<f32> };

@vertex fn vs_tri(v: VIn) -> VOut {
    var o: VOut;
    o.clip = u.view_proj * vec4<f32>(v.pos, 1.0);
    let k = 0.45 + 0.55 * abs(dot(normalize(v.normal), u.light.xyz));
    o.color = vec4<f32>(v.color.rgb * k, v.color.a * u.params.x);
    o.uv = v.uv;
    return o;
}

@vertex fn vs_line(v: VIn) -> VOut {
    var o: VOut;
    o.clip = u.view_proj * vec4<f32>(v.pos, 1.0);
    // Edges slightly closer to the camera so they don't sink into their own part's faces.
    o.clip.z = o.clip.z - u.params.y * o.clip.w;
    o.color = v.color;
    o.uv = vec2<f32>(0.0, 0.0);
    return o;
}

// Color times texture: materials without an image get a white pixel texture.
@fragment fn fs(i: VOut) -> @location(0) vec4<f32> { return i.color * textureSample(tex, samp, i.uv); }
"#;

/// Infinite grid (like the cadcraft canvas): one full-screen triangle; for each pixel — a point
/// on the grid plane (perpendicular to the axis closest to the view direction), lines every
/// `minor` and `major` mm with antialiasing, axes through the origin; depth is real, parts occlude the grid.
const GRID_SHADER: &str = r#"
struct G { eye: vec4<f32>, r: vec4<f32>, u: vec4<f32>, f: vec4<f32>, half: vec4<f32>, grid: vec4<f32>, minor_c: vec4<f32>, major_c: vec4<f32> };
@group(0) @binding(0) var<uniform> g: G;
struct VOut { @builtin(position) clip: vec4<f32>, @location(0) ndc: vec2<f32> };
struct FOut { @location(0) color: vec4<f32>, @builtin(frag_depth) depth: f32 };

@vertex fn vs(@builtin(vertex_index) i: u32) -> VOut {
    let p = vec2<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0);
    var o: VOut;
    o.clip = vec4<f32>(p, 0.5, 1.0);
    o.ndc = p;
    return o;
}

fn line(c: f32, step: f32) -> f32 {
    let x = c / step;
    let w = fwidth(x);
    let d = abs(fract(x - 0.5) - 0.5) / max(w, 1e-6);
    return 1.0 - min(d, 1.0);
}

@fragment fn fs(v: VOut) -> FOut {
    let n = u32(g.grid.z);
    let f = g.f.xyz;
    // Point on the near plane under the pixel and a ray along the view.
    let o = g.eye.xyz + v.ndc.x * g.half.x * g.r.xyz + v.ndc.y * g.half.y * g.u.xyz - f * g.half.z;
    let fn_ = f[n];
    if (abs(fn_) < 1e-4) { discard; }
    let t = (g.grid.w - o[n]) / fn_;
    if (t < 0.0) { discard; }
    let p = o + f * t;
    let a = (n + 1u) % 3u;
    let b = (n + 2u) % 3u;
    let minor = max(line(p[a], g.grid.x), line(p[b], g.grid.x));
    let major = max(line(p[a], g.grid.y), line(p[b], g.grid.y));
    // Axes through the origin: X — red, Y — green, Z — blue (dark, as in cadcraft).
    let axis_colors = array<vec3<f32>, 3>(vec3<f32>(0.34, 0.16, 0.16), vec3<f32>(0.16, 0.29, 0.18), vec3<f32>(0.16, 0.22, 0.38));
    let ax_a = 1.0 - min(abs(p[b]) / max(fwidth(p[b]), 1e-6), 1.0);
    let ax_b = 1.0 - min(abs(p[a]) / max(fwidth(p[a]), 1e-6), 1.0);
    var col = mix(g.minor_c.rgb, g.major_c.rgb, major);
    var alpha = max(minor, major);
    col = mix(col, axis_colors[a], ax_a);
    col = mix(col, axis_colors[b], ax_b);
    alpha = max(alpha, max(ax_a, ax_b));
    if (alpha < 0.01) { discard; }
    var out: FOut;
    out.color = vec4<f32>(col, alpha);
    out.depth = clamp(dot(f, p - g.eye.xyz) * g.half.w + 0.5, 0.0, 1.0);
    return out;
}
"#;

/// Where the grid lies along the view axis: through the origin, but not in front of the model — otherwise when looking
/// from behind, left or below it covers the model. `f` — view direction along this axis.
fn grid_plane(f: f32, lo: f32, hi: f32) -> f32 {
    if f > 0.0 { hi.max(0.0) } else { lo.min(0.0) }
}

/// Window grid: enabled, step (mm), major line — every N-th.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    pub on: bool,
    pub step: f32,
    pub major: u32,
}

impl Default for Grid {
    fn default() -> Self {
        Grid { on: true, step: 10.0, major: 10 }
    }
}

/// Grid step for the zoom, as in cadcraft: multiply by 5 until lines are no closer than `min_px`.
pub fn adaptive(step: f64, px_per_mm: f64, min_px: f64) -> f64 {
    let mut s = step.max(1e-6);
    for _ in 0..40 {
        if s * px_per_mm >= min_px {
            break;
        }
        s *= 5.0;
    }
    s
}

/// Scene buffers on the GPU.
struct Scene {
    tris: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    /// Triangles grouped by image: (texture index in `Viewport::textures` or none, index range).
    batches: Vec<(Option<usize>, std::ops::Range<u32>)>,
    /// Edges, project axes and part axes (x-ray).
    lines: wgpu::Buffer,
    line_count: u32,
    axes_count: u32,
    part_axes_count: u32,
    /// Drilling holes: a line from the entry point to the drilling depth.
    holes_count: u32,
    /// Scene bounds.
    lo: [f32; 3],
    hi: [f32; 3],
    /// Fitting triangles in project coordinates — for click picking: (triangle, object number).
    furniture: Vec<(dowel::panel::Tri, u32)>,
}

struct Target {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    resolve: wgpu::TextureView,
    depth: wgpu::TextureView,
    id: egui::TextureId,
}

struct Gpu {
    grid_pipe: wgpu::RenderPipeline,
    grid_uniforms: wgpu::Buffer,
    grid_bind: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
    bind: wgpu::BindGroup,
    solid: wgpu::RenderPipeline,
    glass: wgpu::RenderPipeline,
    lines_visible: wgpu::RenderPipeline,
    lines_hidden: wgpu::RenderPipeline,
    lines_all: wgpu::RenderPipeline,
    target: Option<Target>,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::BindGroup,
    /// Loaded images: path → bind group (none — image unreadable).
    images: Vec<(String, Option<wgpu::BindGroup>)>,
}

pub struct Viewport {
    pub camera: Camera,
    /// Grid (drawing settings).
    pub grid: Grid,
    pub mode: Mode,
    /// Fittings: model mesh or a box by bounds.
    pub furniture: Vec<FurnView>,
    furniture_built: Vec<FurnView>,
    gpu: Option<Gpu>,
    scene: Option<Scene>,
    /// CPU-side mesh: for click-picking parts and recoloring the selection.
    mesh: Mesh,
    /// Part revision the mesh was built from, and the selection the scene was built with.
    revision: Option<u64>,
    selected: Option<Vec<usize>>,
    /// Parts whose contour failed to build: (name, reason).
    pub errors: Vec<(String, String)>,
    fitted: bool,
    /// View caption in the window corner ("Front", "Top"…).
    pub label: Option<&'static str>,
    /// Material textures: (full material name, image path, tile mm along grain × across).
    pub textures: Vec<(String, String, [f64; 2])>,
    textures_built: Vec<(String, String, [f64; 2])>,
    mode_built: Option<Mode>,
    /// Dragging the selection: accumulated mouse offset (screen pixels) and the offset the scene was built with.
    dragging: Option<Vec2>,
    offset_built: [f32; 3],
    /// A size handle is being dragged: which one (by screen axes) and the accumulated mouse offset (pixels).
    resize: Option<([i8; 2], Vec2)>,
}

impl Default for Viewport {
    fn default() -> Self {
        let (yaw, pitch) = Camera::AXON;
        Viewport {
            camera: Camera { target: [0.0; 3], yaw: yaw.to_radians(), pitch: pitch.to_radians(), mm_per_px: 2.0 },
            grid: Grid::default(),
            mode: Mode::Solid,
            furniture: Vec::new(),
            furniture_built: Vec::new(),
            gpu: None,
            scene: None,
            mesh: Mesh::default(),
            revision: None,
            selected: None,
            errors: Vec::new(),
            fitted: false,
            label: None,
            textures: Vec::new(),
            textures_built: Vec::new(),
            mode_built: None,
            dragging: None,
            resize: None,
            offset_built: [0.0; 3],
        }
    }
}

fn push_vertex(out: &mut Vec<u8>, pos: [f32; 3], normal: [f32; 3], rgba: [f32; 4]) {
    push_vertex_uv(out, pos, normal, rgba, [0.0; 2]);
}

fn push_vertex_uv(out: &mut Vec<u8>, pos: [f32; 3], normal: [f32; 3], rgba: [f32; 4], uv: [f32; 2]) {
    for v in pos.iter().chain(&normal).chain(&rgba).chain(&uv) {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn pipeline(device: &wgpu::Device, layout: &wgpu::PipelineLayout, module: &wgpu::ShaderModule, lines: bool, depth: (bool, wgpu::CompareFunction), blend: bool) -> wgpu::RenderPipeline {
    let attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x2];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("dowel-craft 3d"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(if lines { "vs_line" } else { "vs_tri" }),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout { array_stride: STRIDE, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attributes })],
        },
        primitive: wgpu::PrimitiveState {
            topology: if lines { wgpu::PrimitiveTopology::LineList } else { wgpu::PrimitiveTopology::TriangleList },
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(depth.0),
            depth_compare: Some(depth.1),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState { count: SAMPLES, ..Default::default() },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: COLOR,
                blend: blend.then_some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Infinite grid pipeline: its own shader and its own 128 bytes of parameters.
fn grid_pipeline(device: &wgpu::Device) -> (wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup) {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("grid"), source: wgpu::ShaderSource::Wgsl(GRID_SHADER.into()) });
    let buffer = device.create_buffer(&wgpu::BufferDescriptor { label: Some("grid"), size: 128, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("grid"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        }],
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("grid"), layout: &layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }] });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("grid"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
    let pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("grid"),
        layout: Some(&pl),
        vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::LessEqual), stencil: Default::default(), bias: Default::default() }),
        multisample: wgpu::MultisampleState { count: SAMPLES, ..Default::default() },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState { format: COLOR, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
        }),
        multiview_mask: None,
        cache: None,
    });
    (pipe, buffer, bind)
}

impl Gpu {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Gpu {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("dowel-craft 3d"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniforms"),
            layout: &bind_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() }],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("texture"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white = upload(device, queue, &tex_layout, &sampler, 1, 1, &[255, 255, 255, 255]);
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("3d"), bind_group_layouts: &[Some(&bind_layout), Some(&tex_layout)], immediate_size: 0 });
        use wgpu::CompareFunction::{Always, Greater, LessEqual};
        let (grid_pipe, grid_uniforms, grid_bind) = grid_pipeline(device);
        Gpu {
            grid_pipe,
            grid_uniforms,
            grid_bind,
            solid: pipeline(device, &layout, &module, false, (true, LessEqual), false),
            glass: pipeline(device, &layout, &module, false, (false, Always), true),
            lines_visible: pipeline(device, &layout, &module, true, (false, LessEqual), true),
            lines_hidden: pipeline(device, &layout, &module, true, (false, Greater), true),
            lines_all: pipeline(device, &layout, &module, true, (false, Always), true),
            uniforms,
            bind,
            target: None,
            tex_layout,
            sampler,
            white,
            images: Vec::new(),
        }
    }

    /// An already loaded image (otherwise white).
    fn image_ref(&self, path: &str) -> &wgpu::BindGroup {
        self.images.iter().find(|(p, _)| p == path).and_then(|(_, b)| b.as_ref()).unwrap_or(&self.white)
    }

    /// Image bind group by path (read once; unreadable — white).
    fn image(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, path: &str) -> &wgpu::BindGroup {
        if !self.images.iter().any(|(p, _)| p == path) {
            let bg = image::open(path).ok().map(|img| {
                let rgba = img.to_rgba8();
                upload(device, queue, &self.tex_layout, &self.sampler, rgba.width(), rgba.height(), rgba.as_raw())
            });
            self.images.push((path.to_string(), bg));
        }
        let i = self.images.iter().position(|(p, _)| p == path).expect("добавлена выше");
        self.images[i].1.as_ref().unwrap_or(&self.white)
    }

    /// Textures sized to the window; on resize — new ones, under the same egui texture id.
    fn ensure_target(&mut self, rs: &egui_wgpu::RenderState, size: [u32; 2]) {
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            let tex = |samples: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
                rs.device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("3d"),
                        size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: samples,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            let msaa = tex(SAMPLES, COLOR, wgpu::TextureUsages::RENDER_ATTACHMENT);
            let resolve = tex(1, COLOR, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING);
            let depth = tex(SAMPLES, DEPTH, wgpu::TextureUsages::RENDER_ATTACHMENT);
            let mut renderer = rs.renderer.write();
            let id = match self.target.take() {
                Some(old) => {
                    renderer.update_egui_texture_from_wgpu_texture(&rs.device, &resolve, wgpu::FilterMode::Linear, old.id);
                    old.id
                }
                None => renderer.register_native_texture(&rs.device, &resolve, wgpu::FilterMode::Linear),
            };
            self.target = Some(Target { size, msaa, resolve, depth, id });
        }
    }
}

/// RGBA image → GPU texture and its bind group.
fn upload(device: &wgpu::Device, queue: &wgpu::Queue, layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler, w: u32, h: u32, rgba: &[u8]) -> wgpu::BindGroup {
    let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("material"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        // No sRGB conversion: scene colors are already sRGB (like part colors), otherwise the decor gets darker
        // and more saturated (light yellow turns orange).
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        rgba,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: Some(h) },
        size,
    );
    let view = tex.create_view(&Default::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material"),
        layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) }],
    })
}

impl Scene {
    /// `look` — how to color: "Textured" — the part's material image (index, tile mm), "Wireframe" — edges in part color.
    fn new(device: &wgpu::Device, parts: &[Part], m: &Mesh, selected: &[usize], furniture: &[FurnView], look: Look, offset: [f32; 3]) -> Scene {
        let (textures, wire) = match look {
            Look::Textured(t) => (Some(t), false),
            Look::Wire => (None, true),
            Look::Plain => (None, false),
        };
        // While the selection is being dragged it is drawn shifted by `offset`.
        let moved = |part: usize, p: [f32; 3]| if selected.contains(&part) { std::array::from_fn(|k| p[k] + offset[k]) } else { p };
        let mut tris = Vec::with_capacity(m.vertices.len() * STRIDE as usize);
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        let tex_of = |part: usize| textures.and_then(|t| t.iter().position(|(mat, _, _)| *mat == parts[part].panel.material));
        for v in &m.vertices {
            let part = v.part as usize;
            let tex = tex_of(part);
            let c = if selected.contains(&part) {
                SELECTED
            } else if tex.is_some() {
                [1.0, 1.0, 1.0]
            } else {
                color(m.kinds[part])
            };
            // Grain runs along the tile length: for a vertical texture — along contour Y.
            let uv = match (tex, textures) {
                (Some(i), Some(t)) => {
                    let tile = t[i].2;
                    let vertical = parts[part].panel.texture == dowel::panel::Texture::Vertical;
                    let (a, b) = if vertical { (v.uv[1], v.uv[0]) } else { (v.uv[0], v.uv[1]) };
                    [a / tile[0] as f32, b / tile[1] as f32]
                }
                _ => [0.0; 2],
            };
            push_vertex_uv(&mut tris, moved(part, v.pos), v.normal, [c[0], c[1], c[2], 1.0], uv);
            for k in 0..3 {
                lo[k] = lo[k].min(v.pos[k]);
                hi[k] = hi[k].max(v.pos[k]);
            }
        }
        // Bounds including fittings — up to the project axes, their length derived from the bounds.
        for f in furniture {
            for p in f.mesh.iter().flatten() {
                let w = f.frame.apply(p.map(f64::from));
                for k in 0..3 {
                    lo[k] = lo[k].min(w[k] as f32);
                    hi[k] = hi[k].max(w[k] as f32);
                }
            }
        }
        if lo[0] > hi[0] {
            (lo, hi) = ([0.0; 3], [1000.0; 3]);
        }
        let mut lines = Vec::new();
        // Dark edges over faces; in "Wireframe" there are no faces and the background is dark — edges in part color.
        let edge_of = |part: usize| match (wire, selected.contains(&part)) {
            (false, _) => [0.15, 0.15, 0.15, 1.0],
            (true, true) => [SELECTED[0], SELECTED[1], SELECTED[2], 1.0],
            (true, false) => {
                let c = color(m.kinds[part]);
                [c[0], c[1], c[2], 1.0]
            }
        };
        for (a, b, part) in &m.lines {
            let edge = edge_of(*part as usize);
            push_vertex(&mut lines, moved(*part as usize, *a), [0.0; 3], edge);
            push_vertex(&mut lines, moved(*part as usize, *b), [0.0; 3], edge);
        }
        let line_count = m.lines.len() as u32 * 2;
        // Project axes at the origin: X — red, Y — green, Z — blue.
        let len = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f32::max) * 0.15;
        let axes = [([len, 0.0, 0.0], [0.85, 0.1, 0.1, 1.0]), ([0.0, len, 0.0], [0.1, 0.6, 0.1, 1.0]), ([0.0, 0.0, len], [0.1, 0.25, 0.9, 1.0])];
        for (end, c) in axes {
            push_vertex(&mut lines, [0.0; 3], [0.0; 3], c);
            push_vertex(&mut lines, end, [0.0; 3], c);
        }
        // Each part's axes (for x-ray): from its coordinate origin, 60 mm.
        let mut part_axes = 0;
        for p in parts {
            let Some(f) = p.panel.frame else { continue };
            let o = f.origin.map(|v| v as f32);
            for (dir, c) in [(f.x, [0.85, 0.1, 0.1, 1.0]), (f.y, [0.1, 0.6, 0.1, 1.0]), (f.z, [0.1, 0.25, 0.9, 1.0])] {
                push_vertex(&mut lines, o, [0.0; 3], c);
                push_vertex(&mut lines, std::array::from_fn(|k| o[k] + dir[k] as f32 * 60.0), [0.0; 3], c);
                part_axes += 2;
            }
        }
        // Holes: a red line from the entry along the drilling direction to the depth.
        let mut holes = 0;
        for p in parts {
            let Some(f) = p.panel.frame else { continue };
            for h in &p.panel.holes {
                let a = f.apply(h.pos);
                let b = f.apply(std::array::from_fn(|k| h.pos[k] + h.dir[k] * h.depth));
                push_vertex(&mut lines, a.map(|v| v as f32), [0.0; 3], [0.85, 0.05, 0.05, 1.0]);
                push_vertex(&mut lines, b.map(|v| v as f32), [0.0; 3], [0.85, 0.05, 0.05, 1.0]);
                holes += 2;
            }
        }
        // Fittings without a mesh — a box by bounds (selected — orange).
        for FurnView { frame: fr, lo, hi, selected: sel, .. } in furniture.iter().filter(|f| f.mesh.is_empty()) {
            let c = if *sel { [1.0, 0.55, 0.1, 1.0] } else { [0.15, 0.35, 0.85, 1.0] };
            let corner = |i: usize| fr.apply(std::array::from_fn(|k| if i >> k & 1 == 0 { lo[k] } else { hi[k] })).map(|v| v as f32);
            for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
                push_vertex(&mut lines, corner(a), [0.0; 3], c);
                push_vertex(&mut lines, corner(b), [0.0; 3], c);
                holes += 2;
            }
        }
        // Triangles — in batches per image.
        let mut order: Vec<usize> = (0..m.indices.len() / 3).collect();
        order.sort_by_key(|&t| tex_of(m.vertices[m.indices[3 * t] as usize].part as usize).map_or(0, |i| i + 1));
        let mut batches: Vec<(Option<usize>, std::ops::Range<u32>)> = Vec::new();
        let mut sorted = Vec::with_capacity(m.indices.len());
        for t in order {
            let key = tex_of(m.vertices[m.indices[3 * t] as usize].part as usize);
            let at = sorted.len() as u32;
            match batches.last_mut() {
                Some((k, r)) if *k == key => r.end = at + 3,
                _ => batches.push((key, at..at + 3)),
            }
            sorted.extend_from_slice(&m.indices[3 * t..3 * t + 3]);
        }
        // Fittings with a mesh — their own triangles (metal gray, selected — orange), no image.
        let mut furn_tris = Vec::new();
        let mut next = m.vertices.len() as u32;
        let start = sorted.len() as u32;
        for f in furniture.iter().filter(|f| !f.mesh.is_empty()) {
            let c = if f.selected { [SELECTED[0], SELECTED[1], SELECTED[2], 1.0] } else { [0.62, 0.64, 0.68, 1.0] };
            for t in f.mesh.iter() {
                let w: dowel::panel::Tri = t.map(|p| f.frame.apply(p.map(f64::from)).map(|v| v as f32));
                let (a, b) = (std::array::from_fn::<f32, 3, _>(|k| w[1][k] - w[0][k]), std::array::from_fn::<f32, 3, _>(|k| w[2][k] - w[0][k]));
                let n = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
                let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
                for p in w {
                    push_vertex_uv(&mut tris, p, n.map(|v| v / l), c, [0.0; 2]);
                    sorted.push(next);
                    next += 1;
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
                furn_tris.push((w, f.id));
            }
        }
        let end = sorted.len() as u32;
        if end > start {
            match batches.last_mut() {
                Some((None, r)) if r.end == start => r.end = end,
                _ => batches.push((None, start..end)),
            }
        }
        let index_bytes: Vec<u8> = sorted.iter().flat_map(|i| i.to_le_bytes()).collect();
        let buf = |data: &[u8], usage| {
            // wgpu doesn't create an empty buffer — put at least 4 bytes.
            let data = if data.is_empty() { &[0u8; 4][..] } else { data };
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("3d"), contents: data, usage })
        };
        Scene {
            tris: buf(&tris, wgpu::BufferUsages::VERTEX),
            indices: buf(&index_bytes, wgpu::BufferUsages::INDEX),
            index_count: sorted.len() as u32,
            batches,
            lines: buf(&lines, wgpu::BufferUsages::VERTEX),
            line_count,
            axes_count: 6,
            part_axes_count: part_axes,
            holes_count: holes,
            furniture: furn_tris,
            lo,
            hi,
        }
    }
}

impl Viewport {
    /// Fit the scene into a window of `size` pixels.
    fn fit(&mut self, size: Vec2) {
        let Some(s) = &self.scene else { return };
        self.camera.target = std::array::from_fn(|k| (s.lo[k] + s.hi[k]) / 2.0);
        let diag = (0..3).map(|k| (s.hi[k] - s.lo[k]).powi(2)).sum::<f32>().sqrt().max(100.0);
        self.camera.mm_per_px = diag / size.x.min(size.y).max(50.0) * 1.1;
    }

    /// A window with a fixed view (for "4 windows"): angle in degrees, corner caption.
    pub fn with_view(yaw: f32, pitch: f32, label: &'static str) -> Viewport {
        let mut v = Viewport::default();
        (v.camera.yaw, v.camera.pitch) = (yaw.to_radians(), pitch.to_radians());
        v.label = Some(label);
        v
    }

    /// Refit the scene on the next show.
    pub fn refit(&mut self) {
        self.fitted = false;
    }

    /// Whether the window is waiting for a fit ("Show all" was pressed).
    pub fn needs_fit(&self) -> bool {
        !self.fitted
    }

    /// Camera at angles (degrees): rotation about the vertical and tilt.
    pub fn set_view(&mut self, yaw: f32, pitch: f32) {
        (self.camera.yaw, self.camera.pitch) = (yaw.to_radians(), pitch.to_radians());
    }

    /// View and mode bar above the model window — icons, as in cadcraft.
    pub fn toolbar(&mut self, ui: &mut egui::Ui) {
        use crate::icons::{Icon, button};
        const S: f32 = 26.0;
        let views = [(Icon::ViewFront, "Вид спереди (1)", 0.0, 0.0), (Icon::ViewLeft, "Вид слева (2)", 90.0, 0.0), (Icon::ViewTop, "Вид сверху (3)", 0.0, 89.9), (Icon::ViewIso, "Аксонометрия, объёмный вид (4)", Camera::AXON.0, Camera::AXON.1)];
        ui.weak(crate::i18n::t("Вид"));
        for (icon, tip, yaw, pitch) in views {
            let on = (self.camera.yaw - f32::to_radians(yaw)).abs() < 1e-3 && (self.camera.pitch - f32::to_radians(pitch)).abs() < 1e-3;
            if button(ui, icon, S, tip, on).clicked() {
                (self.camera.yaw, self.camera.pitch) = (f32::to_radians(yaw), f32::to_radians(pitch));
            }
        }
        if button(ui, Icon::Fit, S, "Показать всё (5): вписать модель в окно", false).clicked() {
            self.fitted = false;
        }
        ui.separator();
        ui.weak(crate::i18n::t("Показ"));
        let icons = [Icon::ModeSolid, Icon::ModeHidden, Icon::ModeWire, Icon::ModeXRay, Icon::ModeTextured];
        let tips = [
            "Без невидимых (Alt+1): детали сплошные, видны только ближние рёбра",
            "Невидимые тонкие (Alt+2): как сплошной, плюс скрытые рёбра светлыми линиями",
            "Каркас (Alt+3): только рёбра деталей, насквозь",
            "Прозрачность, рентген (Alt+4): детали полупрозрачные, видны оси каждой детали",
            "В текстурах (Alt+5): материалы картинками (Балтламинат по артикулу)",
        ];
        for (((mode, _), icon), tip) in Mode::ALL.into_iter().zip(icons).zip(tips) {
            if button(ui, icon, S, tip, self.mode == mode).clicked() {
                self.mode = mode;
            }
        }
    }

    /// Offset of the dragged selection: along the project axis the mouse moves along on screen, whole mm.
    /// World axes pointing right and up on screen for an unrotated view ("Front", "Left", "Top").
    fn screen_axes(&self) -> Option<[usize; 2]> {
        let (_, r, u) = self.camera.basis();
        let axis = |v: [f32; 3]| (0..3).find(|&k| v[k].abs() > 0.999);
        Some([axis(r)?, axis(u)?])
    }

    /// Project point on screen: offset from the window center, egui points.
    fn to_screen(&self, p: [f64; 3]) -> Vec2 {
        let (_, r, u) = self.camera.basis();
        let d: [f32; 3] = std::array::from_fn(|k| p[k] as f32 - self.camera.target[k]);
        let dot = |v: [f32; 3]| (0..3).map(|k| d[k] * v[k]).sum::<f32>();
        Vec2::new(dot(r), -dot(u)) / self.camera.mm_per_px
    }

    /// Mouse offset `m` (pixels) to mm along screen axes `axes`.
    fn screen_mm(&self, axes: [usize; 2], m: Vec2) -> [f64; 2] {
        let (_, r, u) = self.camera.basis();
        let k = f64::from(self.camera.mm_per_px);
        [f64::from(m.x * r[axes[0]].signum()) * k, -f64::from(m.y * u[axes[1]].signum()) * k]
    }

    fn drag_offset(&self) -> [f64; 3] {
        let Some(d) = self.dragging else { return [0.0; 3] };
        let (_, r, u) = self.camera.basis();
        // Screen direction of axis k per 1 mm (pixels, y down).
        let screen = |k: usize| Vec2::new(r[k], -u[k]) / self.camera.mm_per_px;
        let best = (0..3).filter(|&k| screen(k).length() > 0.25 / self.camera.mm_per_px).max_by(|&a, &b| {
            let c = |k: usize| (d.dot(screen(k)) / screen(k).length()).abs();
            c(a).total_cmp(&c(b))
        });
        let mut out = [0.0; 3];
        if let Some(k) = best {
            let s = screen(k);
            out[k] = f64::from(d.dot(s) / s.length_sq()).round();
        }
        out
    }

    /// Part under a screen point (offset from the window center in egui points).
    /// Nearest thing under a screen point: a part (`Ok`) or a fitting (`Err` — object number).
    fn pick(&self, off: Vec2) -> Option<Result<usize, u32>> {
        let s = self.scene.as_ref()?;
        let depth = (0..3).map(|k| (s.hi[k] - s.lo[k]).abs()).sum::<f32>() * 4.0 + 1000.0;
        let (o, d) = self.camera.ray(off, depth);
        let m = &self.mesh;
        let parts = m.indices.as_chunks::<3>().0.iter().filter_map(|t| {
            let tri = [0, 1, 2].map(|i| m.vertices[t[i] as usize].pos);
            Some((hit(o, d, &tri)?, Ok(m.vertices[t[0] as usize].part as usize)))
        });
        let furniture = s.furniture.iter().filter_map(|(tri, id)| Some((hit(o, d, tri)?, Err(*id))));
        parts.chain(furniture).min_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, h)| h)
    }

    /// Model window. `revision` changes when parts change — then the mesh is rebuilt.
    /// Returns the click (on a part, on empty space, right button) and the window response — the menu attaches to it.
    pub fn show(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame, parts: &[Part], revision: u64, selected: &[usize], skip: impl Fn(&Part) -> bool) -> (Click, Option<egui::Response>) {
        let Some(rs) = frame.wgpu_render_state() else {
            ui.label(crate::i18n::t("3D недоступно: окно запущено без видеокарты (wgpu)."));
            return (Click::None, None);
        };
        if self.gpu.is_none() {
            self.gpu = Some(Gpu::new(&rs.device, &rs.queue));
        }
        if self.revision != Some(revision) {
            self.mesh = mesh::build(parts, skip);
            self.errors = self.mesh.errors.iter().map(|(i, e)| (parts[*i].panel.name.clone(), e.clone())).collect();
            if self.revision.is_none() {
                self.fitted = false;
            }
            self.revision = Some(revision);
            self.selected = None;
        }
        let textured = self.mode == Mode::Textured;
        let offset = self.drag_offset().map(|v| v as f32);
        if self.offset_built != offset || self.selected.as_deref() != Some(selected) || self.furniture_built != self.furniture || self.textures_built != self.textures || self.mode_built != Some(self.mode) {
            let look = match self.mode {
                Mode::Textured => Look::Textured(&self.textures),
                Mode::Wire => Look::Wire,
                Mode::Solid | Mode::Hidden | Mode::XRay => Look::Plain,
            };
            self.scene = Some(Scene::new(&rs.device, parts, &self.mesh, selected, &self.furniture, look, offset));
            self.offset_built = offset;
            self.selected = Some(selected.to_vec());
            self.furniture_built = self.furniture.clone();
            self.textures_built = self.textures.clone();
            self.mode_built = Some(self.mode);
        }

        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        if !self.fitted {
            self.fit(rect.size());
            self.fitted = true;
        }
        // LMB — rotate, RMB — pan, wheel — zoom about the point under the cursor.
        let mut click = Click::None;
        if resp.clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            click = match self.pick(pos - rect.center()) {
                Some(Ok(i)) => Click::Part(i),
                Some(Err(id)) => Click::Furniture(id, false),
                None => Click::Empty,
            };
        }
        if resp.secondary_clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            click = match self.pick(pos - rect.center()) {
                Some(Err(id)) => Click::Furniture(id, true),
                hit => Click::Menu(hit.and_then(Result::ok)),
            };
        }
        // Size handles, as in the front view: in unrotated windows, for a single selected rectangular panel.
        let sized = match (self.label, self.screen_axes(), selected) {
            (Some(_), Some(axes), &[i]) if parts[i].id != 0 && parts[i].panel.is_rect() => Some((i, axes)),
            _ => None,
        };
        let mut markers = Vec::new();
        if let Some((i, axes)) = sized {
            let f = parts[i].panel.frame.unwrap_or(dowel::geom::Frame::IDENTITY);
            let d = self.resize.map_or([0.0; 2], |(_, m)| self.screen_mm(axes, m));
            let (lo, hi) = resized(parts, i, axes, self.resize.map_or([0; 2], |(h, _)| h), d, 10.0 * f64::from(self.camera.mm_per_px));
            for hx in [-1i8, 0, 1] {
                for hy in [-1i8, 0, 1] {
                    // Size can't be dragged along the thickness.
                    if (hx, hy) == (0, 0) || (hx != 0 && f.z[axes[0]].abs() > 0.5) || (hy != 0 && f.z[axes[1]].abs() > 0.5) {
                        continue;
                    }
                    let mut p = lo;
                    for (n, h) in [hx, hy].into_iter().enumerate() {
                        let k = axes[n];
                        p[k] = match h { -1 => lo[k], 1 => hi[k], _ => (lo[k] + hi[k]) / 2.0 };
                    }
                    // The part is being dragged — handles move with it.
                    let mv = self.drag_offset();
                    markers.push(([hx, hy], rect.center() + self.to_screen(std::array::from_fn(|k| p[k] + mv[k]))));
                }
            }
            if resp.drag_started_by(egui::PointerButton::Primary)
                && let Some(o) = ui.input(|i| i.pointer.press_origin())
                && let Some((h, _)) = markers.iter().find(|(_, m)| (o - *m).abs().max_elem() < 7.0)
            {
                self.resize = Some((*h, Vec2::ZERO));
            }
            if let Some((h, m)) = &mut self.resize {
                *m += resp.drag_delta();
                if !resp.dragged() {
                    let (h, m) = (*h, *m);
                    self.resize = None;
                    let (a, b) = resized(parts, i, axes, h, self.screen_mm(axes, m), 10.0 * f64::from(self.camera.mm_per_px));
                    if (a, b) != (parts[i].origin, std::array::from_fn(|k| parts[i].origin[k] + parts[i].size[k])) {
                        click = Click::Resize(a, b);
                    }
                }
            }
            if self.resize.is_some() {
                let (pa, pb) = (rect.center() + self.to_screen(lo), rect.center() + self.to_screen(hi));
                ui.painter().rect_stroke(egui::Rect::from_two_pos(pa, pb), 0.0, egui::Stroke::new(1.5, Color32::from_rgb(220, 110, 0)), egui::StrokeKind::Middle);
                ui.painter().text(egui::Rect::from_two_pos(pa, pb).center_top() - Vec2::new(0.0, 4.0), egui::Align2::CENTER_BOTTOM, format!("{} × {}", hi[axes[0]] - lo[axes[0]], hi[axes[1]] - lo[axes[1]]), egui::FontId::proportional(13.0), Color32::from_rgb(230, 140, 40));
            }
        } else {
            self.resize = None;
        }
        // Drag started on a selected part — move it; on empty space or another part — rotate the view.
        if self.resize.is_none()
            && resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(o) = ui.input(|i| i.pointer.press_origin())
            && matches!(self.pick(o - rect.center()), Some(Ok(i)) if selected.contains(&i))
        {
            self.dragging = Some(Vec2::ZERO);
        }
        if let Some(d) = &mut self.dragging {
            if resp.dragged_by(egui::PointerButton::Primary) {
                *d += resp.drag_delta();
            }
            if !resp.dragged() {
                let mv = self.drag_offset();
                self.dragging = None;
                if mv != [0.0; 3] {
                    click = Click::Move(mv);
                }
            }
        } else if self.resize.is_some() {
        } else if resp.dragged_by(egui::PointerButton::Primary) && self.label.is_some() {
            // "Front", "Left", "Top" windows (4 views) don't rotate: dragging pans the view.
            self.camera.pan(resp.drag_delta());
        } else if resp.dragged_by(egui::PointerButton::Primary) {
            let d = resp.drag_delta();
            self.camera.yaw += d.x * 0.01;
            self.camera.pitch = (self.camera.pitch + d.y * 0.01).clamp(-1.57, 1.57);
        }
        if resp.dragged_by(egui::PointerButton::Secondary) || resp.dragged_by(egui::PointerButton::Middle) {
            self.camera.pan(resp.drag_delta());
        }
        if let Some(pos) = resp.hover_pos() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let k = (-scroll * 0.002).exp();
                let off = pos - rect.center();
                // The point under the cursor stays put: pan by a fraction of the offset from the center.
                self.camera.pan(-off * (1.0 - k));
                self.camera.mm_per_px *= k;
            }
        }

        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32];
        let Some(scene) = &self.scene else { return (click, Some(resp)) };
        let depth = (0..3).map(|k| (scene.hi[k] - scene.lo[k]).powi(2)).sum::<f32>().sqrt().max(100.0) * 2.0
            + (0..3).map(|k| (self.camera.target[k] - (scene.lo[k] + scene.hi[k]) / 2.0).powi(2)).sum::<f32>().sqrt();
        let mut cam = self.camera;
        cam.mm_per_px /= ppp;
        let vp = cam.view_proj(size[0] as f32, size[1] as f32, depth);
        let (f, _, _) = cam.basis();
        let alpha = if self.mode == Mode::XRay { 0.22 } else { 1.0 };
        let mut u: Vec<u8> = vp.iter().flatten().flat_map(|v| v.to_le_bytes()).collect();
        for v in [-f[0], -f[1], -f[2], 0.0, alpha, 0.0005, 0.0, 0.0] {
            u.extend_from_slice(&f32::to_le_bytes(v));
        }

        let Some(gpu) = self.gpu.as_mut() else { return (click, Some(resp)) };
        gpu.ensure_target(rs, size);
        if textured {
            for (_, path, _) in &self.textures {
                gpu.image(&rs.device, &rs.queue, path);
            }
        }
        rs.queue.write_buffer(&gpu.uniforms, 0, &u);
        {
            let (f, r, up) = cam.basis();
            let n = (0..3).max_by(|&i, &j| f[i].abs().total_cmp(&f[j].abs())).unwrap_or(1);
            let minor = adaptive(f64::from(self.grid.step), f64::from(1.0 / self.camera.mm_per_px), 8.0) as f32;
            let major = minor * self.grid.major.max(1) as f32;
            let c = |rgb: [u8; 3]| rgb.map(|v| f32::from(v) / 255.0);
            let (cmin, cmaj) = (c([0x31, 0x39, 0x46]), c([0x45, 0x50, 0x60]));
            let t = cam.target;
            let vals: [f32; 32] = [
                t[0], t[1], t[2], 0.0, r[0], r[1], r[2], 0.0, up[0], up[1], up[2], 0.0, f[0], f[1], f[2], 0.0,
                size[0] as f32 / 2.0 * cam.mm_per_px, size[1] as f32 / 2.0 * cam.mm_per_px, depth, 0.5 / depth,
                minor, major, n as f32, grid_plane(f[n], scene.lo[n], scene.hi[n]),
                cmin[0], cmin[1], cmin[2], 1.0, cmaj[0], cmaj[1], cmaj[2], 1.0,
            ];
            let bytes: Vec<u8> = vals.iter().flat_map(|v| v.to_le_bytes()).collect();
            rs.queue.write_buffer(&gpu.grid_uniforms, 0, &bytes);
        }
        let Some(target) = &gpu.target else { return (click, Some(resp)) };
        let mut enc = rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("3d") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("3d"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.msaa,
                    depth_slice: None,
                    resolve_target: Some(&target.resolve),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1333, g: 0.1569, b: 0.1843, a: 1.0 }), store: wgpu::StoreOp::Discard },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &gpu.bind, &[]);
            pass.set_bind_group(1, &gpu.white, &[]);
            let edges = 0..scene.line_count;
            let axes = scene.line_count..scene.line_count + scene.axes_count;
            let part_axes = axes.end..axes.end + scene.part_axes_count;
            let holes = part_axes.end..part_axes.end + scene.holes_count;
            // Grid first and with depth: parts occlude it.
            if self.grid.on {
                pass.set_pipeline(&gpu.grid_pipe);
                pass.set_bind_group(0, &gpu.grid_bind, &[]);
                pass.draw(0..3, 0..1);
                pass.set_bind_group(0, &gpu.bind, &[]);
            }
            match self.mode {
                Mode::Solid | Mode::Hidden => {
                    pass.set_pipeline(&gpu.solid);
                    pass.set_vertex_buffer(0, scene.tris.slice(..));
                    pass.set_index_buffer(scene.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..scene.index_count, 0, 0..1);
                    pass.set_vertex_buffer(0, scene.lines.slice(..));
                    if self.mode == Mode::Hidden {
                        // shortcut: hidden edges as a light solid line, not dashed.
                        pass.set_pipeline(&gpu.lines_hidden);
                        pass.draw(edges.clone(), 0..1);
                    }
                    pass.set_pipeline(&gpu.lines_visible);
                    pass.draw(edges, 0..1);
                    pass.draw(holes.clone(), 0..1);
                }
                Mode::Textured => {
                    pass.set_pipeline(&gpu.solid);
                    pass.set_vertex_buffer(0, scene.tris.slice(..));
                    pass.set_index_buffer(scene.indices.slice(..), wgpu::IndexFormat::Uint32);
                    for (tex, range) in &scene.batches {
                        let group = tex.and_then(|i| self.textures.get(i)).map_or(&gpu.white, |(_, path, _)| gpu.image_ref(path));
                        pass.set_bind_group(1, group, &[]);
                        pass.draw_indexed(range.clone(), 0, 0..1);
                    }
                    pass.set_bind_group(1, &gpu.white, &[]);
                    pass.set_vertex_buffer(0, scene.lines.slice(..));
                    pass.set_pipeline(&gpu.lines_visible);
                    pass.draw(edges, 0..1);
                }
                Mode::Wire => {
                    pass.set_vertex_buffer(0, scene.lines.slice(..));
                    pass.set_pipeline(&gpu.lines_all);
                    pass.draw(edges, 0..1);
                }
                Mode::XRay => {
                    pass.set_pipeline(&gpu.glass);
                    pass.set_vertex_buffer(0, scene.tris.slice(..));
                    pass.set_index_buffer(scene.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..scene.index_count, 0, 0..1);
                    pass.set_vertex_buffer(0, scene.lines.slice(..));
                    pass.set_pipeline(&gpu.lines_all);
                    pass.draw(edges, 0..1);
                    pass.draw(part_axes, 0..1);
                    pass.draw(holes.clone(), 0..1);
                }
            }
            pass.set_pipeline(&gpu.lines_all);
            pass.set_vertex_buffer(0, scene.lines.slice(..));
            pass.draw(axes, 0..1);
        }
        rs.queue.submit([enc.finish()]);
        ui.painter().image(target.id, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        if self.dragging.is_some() {
            let mv = self.drag_offset();
            let text = ["X", "Y", "Z"].iter().zip(mv).find(|(_, v)| *v != 0.0).map_or_else(|| crate::i18n::t("сдвиг: 0"), |(a, v)| crate::i18n::t(format!("сдвиг по {a}: {v} мм")));
            ui.painter().text(rect.left_top() + Vec2::new(10.0, 28.0), egui::Align2::LEFT_TOP, text, egui::FontId::proportional(15.0), Color32::from_rgb(0xf5, 0x9e, 0x3b));
        }
        for (_, m) in &markers {
            let hr = egui::Rect::from_center_size(*m, Vec2::splat(9.0));
            ui.painter().rect(hr, 1.0, Color32::WHITE, egui::Stroke::new(1.2, Color32::from_rgb(220, 110, 0)), egui::StrokeKind::Inside);
        }
        if let Some(l) = self.label {
            ui.painter().text(rect.left_top() + Vec2::new(6.0, 4.0), egui::Align2::LEFT_TOP, crate::i18n::t(l), egui::FontId::proportional(13.0), Color32::from_gray(90));
            ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(1.0, Color32::from_gray(150)), egui::StrokeKind::Inside);
        }
        if !self.errors.is_empty() {
            let text = format!("Не построены контуры деталей ({}): {}", self.errors.len(), self.errors.iter().map(|(n, e)| format!("«{n}» — {e}")).collect::<Vec<_>>().join("; "));
            ui.painter().text(rect.left_bottom() + Vec2::new(8.0, -8.0), egui::Align2::LEFT_BOTTOM, text, egui::FontId::proportional(13.0), Color32::from_rgb(200, 40, 40));
        }
        (click, Some(resp))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drag(yaw: f32, pitch: f32, d: Vec2) -> [f64; 3] {
        let mut v = Viewport::default();
        v.set_view(yaw, pitch);
        v.camera.mm_per_px = 2.0;
        v.dragging = Some(d);
        v.drag_offset()
    }

    /// The grid is always behind the model: from the front — the plane through the origin (cabinet back), from behind — behind its front.
    #[test]
    fn grid_stays_behind_the_model() {
        // Cabinet depth 0…300. Looking from the front (towards −Z): grid at 0. From behind (towards +Z): at 300.
        assert_eq!(grid_plane(-1.0, 0.0, 300.0), 0.0);
        assert_eq!(grid_plane(1.0, 0.0, 300.0), 300.0);
        // Model entirely behind the origin — the grid stays at zero.
        assert_eq!(grid_plane(1.0, -500.0, -100.0), 0.0);
    }

    /// Size handle: the edge follows the mouse offset and snaps to a neighbouring part's edge.
    #[test]
    fn marker_resizes_and_snaps() {
        let mut s = dowel::design::Session::new(dowel::design::Project::new("т"));
        s.execute("material.add", &serde_json::json!({"name": "ЛДСП 16", "thickness": 16})).unwrap();
        s.execute("panel.add", &serde_json::json!({"material": "ЛДСП 16", "plane": "front", "size": [400, 600], "position": [0, 0, 0]})).unwrap();
        s.execute("panel.add", &serde_json::json!({"material": "ЛДСП 16", "plane": "vertical", "size": [300, 700], "position": [500, 0, 0]})).unwrap();
        let parts = dowel::design::build(&s.project).unwrap();
        // Right edge of the first panel (X) by +93 mm: 493 snaps to 500 (within 10 mm).
        let (lo, hi) = resized(&parts, 0, [0, 1], [1, 0], [93.0, 0.0], 10.0);
        assert_eq!((lo, hi), ([0.0, 0.0, 0.0], [500.0, 600.0, 16.0]));
        // Top by +50.4 mm: no neighbours nearby — whole mm.
        let (_, hi) = resized(&parts, 0, [0, 1], [0, 1], [0.0, 50.4], 10.0);
        assert_eq!(hi[1], 650.0);
    }

    /// Dragging the selection: offset goes along one project axis — the one the mouse moves along on screen.
    #[test]
    fn drag_moves_along_the_screen_axis() {
        // Front: right — +X, up — +Y; 100 pixels at 2 mm/pixel — 200 mm.
        assert_eq!(drag(0.0, 0.0, Vec2::new(100.0, 0.0)), [200.0, 0.0, 0.0]);
        assert_eq!(drag(0.0, 0.0, Vec2::new(5.0, -100.0)), [0.0, 200.0, 0.0]);
        // Top: down the screen — towards the viewer, +Z.
        let top = drag(0.0, 89.9, Vec2::new(0.0, 50.0));
        assert_eq!((top[0], top[1]), (0.0, 0.0));
        assert!(top[2] > 0.0, "{top:?}");
        // Axonometry: horizontally right — along X.
        let iso = drag(Camera::AXON.0, Camera::AXON.1, Vec2::new(80.0, 0.0));
        assert!(iso[0] > 0.0 && iso[1] == 0.0 && iso[2] == 0.0, "{iso:?}");
    }
}
