//! Окно модели: 3D на видеокарте (wgpu), как в Базисе — режимы «Без невидимых», «Невидимые тонкие»,
//! «Каркас», «Прозрачность»; виды спереди, слева, сверху, аксонометрия; ЛКМ вращает, ПКМ сдвигает,
//! колесо масштабирует. Рисуется в свою текстуру (со сглаживанием и глубиной) и показывается картинкой.

use basis::design::{Kind, Part};
use basis::mesh::{self, Mesh};
use eframe::egui::{self, Color32, Vec2};
use eframe::egui_wgpu::{self, wgpu};
use wgpu::util::DeviceExt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// «Без невидимых»: тела и видимые рёбра.
    Solid,
    /// «Невидимые тонкие»: плюс скрытые рёбра светлой линией.
    Hidden,
    /// «Каркас»: только рёбра.
    Wire,
    /// «Прозрачность» (рентген): полупрозрачные тела, все рёбра, оси деталей.
    XRay,
    /// «В текстурах»: материалы картинками (Балтламинат и свои), с видимыми рёбрами.
    Textured,
}

impl Mode {
    pub const ALL: [(Mode, &str); 5] = [(Mode::Solid, "Без невидимых"), (Mode::Hidden, "Невидимые тонкие"), (Mode::Wire, "Каркас"), (Mode::XRay, "Прозрачность"), (Mode::Textured, "В текстурах")];
}

/// Результат щелчка в окне модели.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    None,
    /// Щелчок по детали (индекс в списке деталей).
    Part(usize),
    /// Щелчок по пустому месту — снять выделение.
    Empty,
    /// Правая кнопка (без перетаскивания) — меню; деталь под курсором, если есть.
    Menu(Option<usize>),
    /// Щелчок по фурнитуре (номер объекта); `true` — правой кнопкой.
    Furniture(u32, bool),
}

/// Фурнитура в окне: положение, габарит в своих осях, сетка модели (пустая — рисуется рамкой).
#[derive(Clone, Debug)]
pub struct FurnView {
    pub id: u32,
    pub frame: basis::geom::Frame,
    pub lo: [f64; 3],
    pub hi: [f64; 3],
    pub selected: bool,
    pub mesh: std::sync::Arc<Vec<basis::panel::Tri>>,
}

impl PartialEq for FurnView {
    /// Сетки сравниваются по указателю: одна модель — одна загруженная сетка.
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id && self.frame == o.frame && self.lo == o.lo && self.hi == o.hi && self.selected == o.selected && std::sync::Arc::ptr_eq(&self.mesh, &o.mesh)
    }
}

/// Камера: точка взгляда, углы и масштаб (мм на пиксель), параллельная проекция.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub target: [f32; 3],
    /// Поворот вокруг вертикали, радианы: 0 — смотрим спереди (вдоль −Z).
    pub yaw: f32,
    /// Наклон: положительный — смотрим сверху.
    pub pitch: f32,
    pub mm_per_px: f32,
}

impl Camera {
    /// Аксонометрия Базиса по умолчанию: 20° вокруг OX и −20° вокруг OY.
    pub const AXON: (f32, f32) = (-20.0, 20.0);

    /// Направление «от камеры к сцене», вправо и вверх экрана.
    fn basis(&self) -> ([f32; 3], [f32; 3], [f32; 3]) {
        let (sy, cy, sp, cp) = (self.yaw.sin(), self.yaw.cos(), self.pitch.sin(), self.pitch.cos());
        let forward = [sy * cp, -sp, -cy * cp];
        let right = [cy, 0.0, sy];
        let up = cross(right, forward);
        (forward, right, up)
    }

    /// Матрица «мир → клип» для окна `w`×`h` пикселей; `depth` — полудлина сцены вдоль взгляда.
    fn view_proj(&self, w: f32, h: f32, depth: f32) -> [[f32; 4]; 4] {
        let (f, r, u) = self.basis();
        let (sx, sy) = (2.0 / (w * self.mm_per_px), 2.0 / (h * self.mm_per_px));
        let t = self.target;
        let d = |a: [f32; 3]| -(a[0] * t[0] + a[1] * t[1] + a[2] * t[2]);
        // Глубина 0..1: ближняя плоскость — на `depth` перед точкой взгляда.
        let sz = 0.5 / depth;
        // Столбцы матрицы (WGSL — по столбцам).
        [
            [r[0] * sx, u[0] * sy, f[0] * sz, 0.0],
            [r[1] * sx, u[1] * sy, f[1] * sz, 0.0],
            [r[2] * sx, u[2] * sy, f[2] * sz, 0.0],
            [d(r) * sx, d(u) * sy, d(f) * sz + 0.5, 1.0],
        ]
    }

    /// Луч взгляда через точку экрана, смещённую на `off` пикселей от центра: начало и направление.
    fn ray(&self, off: Vec2, depth: f32) -> ([f32; 3], [f32; 3]) {
        let (f, r, u) = self.basis();
        let o = std::array::from_fn(|k| self.target[k] + (off.x * r[k] - off.y * u[k]) * self.mm_per_px - f[k] * depth);
        (o, f)
    }

    /// Сдвиг точки взгляда на `delta` пикселей экрана.
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

/// Пересечение луча с треугольником (Мёллер — Трумбор): расстояние вдоль луча.
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

/// Цвет выделенной детали.
const SELECTED: [f32; 3] = [1.0, 0.62, 0.2];

/// Цвет детали по назначению: корпус — бежевый, задняя стенка — серая, фасады — голубые.
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
/// Байт на вершину: позиция, нормаль, цвет RGBA, координаты текстуры.
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
    // Рёбра чуть ближе к камере, чтобы не тонули в гранях своей детали.
    o.clip.z = o.clip.z - u.params.y * o.clip.w;
    o.color = v.color;
    o.uv = vec2<f32>(0.0, 0.0);
    return o;
}

// Цвет на текстуру: у материалов без картинки текстура — белая точка.
@fragment fn fs(i: VOut) -> @location(0) vec4<f32> { return i.color * textureSample(tex, samp, i.uv); }
"#;

/// Буферы сцены на видеокарте.
struct Scene {
    tris: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    /// Треугольники по картинкам: (номер текстуры в `Viewport::textures` или нет, диапазон индексов).
    batches: Vec<(Option<usize>, std::ops::Range<u32>)>,
    /// Рёбра, оси проекта и оси деталей (рентген).
    lines: wgpu::Buffer,
    line_count: u32,
    axes_count: u32,
    part_axes_count: u32,
    /// Отверстия присадки: линия сверления от входа на глубину.
    holes_count: u32,
    /// Сетка на полу (Y = 0), как фон в cadcraft: линии через 100 мм, каждая десятая темнее.
    grid_count: u32,
    /// Габарит сцены.
    lo: [f32; 3],
    hi: [f32; 3],
    /// Треугольники фурнитуры в координатах проекта — для выбора щелчком: (треугольник, номер объекта).
    furniture: Vec<(basis::panel::Tri, u32)>,
}

struct Target {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    resolve: wgpu::TextureView,
    depth: wgpu::TextureView,
    id: egui::TextureId,
}

struct Gpu {
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
    /// Загруженные картинки: путь → привязка (нет — картинку не прочитать).
    images: Vec<(String, Option<wgpu::BindGroup>)>,
}

pub struct Viewport {
    pub camera: Camera,
    pub mode: Mode,
    /// Фурнитура: сетка модели или рамка по габариту.
    pub furniture: Vec<FurnView>,
    /// Фурнитура, с которой построена сцена.
    furniture_built: Vec<FurnView>,
    gpu: Option<Gpu>,
    scene: Option<Scene>,
    /// Сетка на процессоре: для выбора деталей щелчком и перекраски выделения.
    mesh: Mesh,
    /// Ревизия деталей, по которой построена сетка, и выделение, с которым построена сцена.
    revision: Option<u64>,
    selected: Option<Vec<usize>>,
    /// Детали, контур которых не построился: (имя, причина).
    pub errors: Vec<(String, String)>,
    fitted: bool,
    /// Подпись вида в углу окна («Спереди», «Сверху»…).
    pub label: Option<&'static str>,
    /// Текстуры материалов: (полное имя материала, путь к картинке, плитка мм вдоль волокон × поперёк).
    pub textures: Vec<(String, String, [f64; 2])>,
    textures_built: Vec<(String, String, [f64; 2])>,
    mode_built: Option<Mode>,
}

impl Default for Viewport {
    fn default() -> Self {
        let (yaw, pitch) = Camera::AXON;
        Viewport {
            camera: Camera { target: [0.0; 3], yaw: yaw.to_radians(), pitch: pitch.to_radians(), mm_per_px: 2.0 },
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
        label: Some("basis-craft 3d"),
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

impl Gpu {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Gpu {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("basis-craft 3d"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
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
        Gpu {
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

    /// Уже загруженная картинка (иначе — белая).
    fn image_ref(&self, path: &str) -> &wgpu::BindGroup {
        self.images.iter().find(|(p, _)| p == path).and_then(|(_, b)| b.as_ref()).unwrap_or(&self.white)
    }

    /// Привязка картинки по пути (читается один раз; нечитаемая — белая).
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

    /// Текстуры под размер окна; при изменении размера — новые, под тем же номером текстуры egui.
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

/// Картинка RGBA → текстура видеокарты и её привязка.
fn upload(device: &wgpu::Device, queue: &wgpu::Queue, layout: &wgpu::BindGroupLayout, sampler: &wgpu::Sampler, w: u32, h: u32, rgba: &[u8]) -> wgpu::BindGroup {
    let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("material"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
    /// `textures` — для режима «В текстурах»: картинка материала детали (номер, плитка мм).
    fn new(device: &wgpu::Device, parts: &[Part], m: &Mesh, selected: &[usize], furniture: &[FurnView], textures: Option<&[(String, String, [f64; 2])]>) -> Scene {
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
            // Волокна — вдоль длины плитки: у вертикальной текстуры — вдоль Y контура.
            let uv = match (tex, textures) {
                (Some(i), Some(t)) => {
                    let tile = t[i].2;
                    let vertical = parts[part].panel.texture == basis::panel::Texture::Vertical;
                    let (a, b) = if vertical { (v.uv[1], v.uv[0]) } else { (v.uv[0], v.uv[1]) };
                    [a / tile[0] as f32, b / tile[1] as f32]
                }
                _ => [0.0; 2],
            };
            push_vertex_uv(&mut tris, v.pos, v.normal, [c[0], c[1], c[2], 1.0], uv);
            for k in 0..3 {
                lo[k] = lo[k].min(v.pos[k]);
                hi[k] = hi[k].max(v.pos[k]);
            }
        }
        // Габарит с фурнитурой — до осей проекта, их длина от габарита.
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
        let edge = [0.15, 0.15, 0.15, 1.0];
        for (a, b, _) in &m.lines {
            push_vertex(&mut lines, *a, [0.0; 3], edge);
            push_vertex(&mut lines, *b, [0.0; 3], edge);
        }
        let line_count = m.lines.len() as u32 * 2;
        // Оси проекта в начале координат: X — красная, Y — зелёная, Z — синяя.
        let len = (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f32::max) * 0.15;
        let axes = [([len, 0.0, 0.0], [0.85, 0.1, 0.1, 1.0]), ([0.0, len, 0.0], [0.1, 0.6, 0.1, 1.0]), ([0.0, 0.0, len], [0.1, 0.25, 0.9, 1.0])];
        for (end, c) in axes {
            push_vertex(&mut lines, [0.0; 3], [0.0; 3], c);
            push_vertex(&mut lines, end, [0.0; 3], c);
        }
        // Оси каждой детали (для рентгена): от начала её системы координат, 60 мм.
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
        // Отверстия: красная линия от входа вдоль сверления на глубину.
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
        // Фурнитура без сетки — рамкой по габариту (выделенная — оранжевой).
        for FurnView { frame: fr, lo, hi, selected: sel, .. } in furniture.iter().filter(|f| f.mesh.is_empty()) {
            let c = if *sel { [1.0, 0.55, 0.1, 1.0] } else { [0.15, 0.35, 0.85, 1.0] };
            let corner = |i: usize| fr.apply(std::array::from_fn(|k| if i >> k & 1 == 0 { lo[k] } else { hi[k] })).map(|v| v as f32);
            for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
                push_vertex(&mut lines, corner(a), [0.0; 3], c);
                push_vertex(&mut lines, corner(b), [0.0; 3], c);
                holes += 2;
            }
        }
        // Сетка пола под изделием с запасом в метр; на больших проектах шаг крупнее.
        let mut grid = 0;
        {
            // Запас 3 м во все стороны — сетка уходит за край окна, как в cadcraft.
            let (x0, x1) = (((lo[0] / 1000.0).floor() - 3.0) * 1000.0, ((hi[0] / 1000.0).ceil() + 3.0) * 1000.0);
            let (z0, z1) = (((lo[2] / 1000.0).floor() - 3.0) * 1000.0, ((hi[2] / 1000.0).ceil() + 3.0) * 1000.0);
            let step = if (x1 - x0).max(z1 - z0) > 16000.0 { 500.0 } else { 100.0 };
            // Цвета cadcraft (тёмный холст): линейные значения, цель — sRGB.
            let color = |v: f32| if (v / 1000.0).fract().abs() < 1e-3 { [0.1022, 0.1329, 0.1878, 1.0] } else { [0.0467, 0.0612, 0.0908, 1.0] };
            let mut v = x0;
            while v <= x1 + 0.5 {
                push_vertex(&mut lines, [v, 0.0, z0], [0.0; 3], color(v));
                push_vertex(&mut lines, [v, 0.0, z1], [0.0; 3], color(v));
                grid += 2;
                v += step;
            }
            let mut v = z0;
            while v <= z1 + 0.5 {
                push_vertex(&mut lines, [x0, 0.0, v], [0.0; 3], color(v));
                push_vertex(&mut lines, [x1, 0.0, v], [0.0; 3], color(v));
                grid += 2;
                v += step;
            }
        }
        // Треугольники — пачками по картинке.
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
        // Фурнитура с сеткой — своими треугольниками (металл серый, выделенная — оранжевая), без картинки.
        let mut furn_tris = Vec::new();
        let mut next = m.vertices.len() as u32;
        let start = sorted.len() as u32;
        for f in furniture.iter().filter(|f| !f.mesh.is_empty()) {
            let c = if f.selected { [SELECTED[0], SELECTED[1], SELECTED[2], 1.0] } else { [0.62, 0.64, 0.68, 1.0] };
            for t in f.mesh.iter() {
                let w: basis::panel::Tri = t.map(|p| f.frame.apply(p.map(f64::from)).map(|v| v as f32));
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
            // Пустой буфер wgpu не создаёт — кладём хотя бы 4 байта.
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
            grid_count: grid,
            furniture: furn_tris,
            lo,
            hi,
        }
    }
}

impl Viewport {
    /// Вписать сцену в окно размером `size` пикселей.
    fn fit(&mut self, size: Vec2) {
        let Some(s) = &self.scene else { return };
        self.camera.target = std::array::from_fn(|k| (s.lo[k] + s.hi[k]) / 2.0);
        let diag = (0..3).map(|k| (s.hi[k] - s.lo[k]).powi(2)).sum::<f32>().sqrt().max(100.0);
        self.camera.mm_per_px = diag / size.x.min(size.y).max(50.0) * 1.1;
    }

    /// Окно с заданным видом (для «4 окон»): угол в градусах, подпись в углу.
    pub fn with_view(yaw: f32, pitch: f32, label: &'static str) -> Viewport {
        let mut v = Viewport::default();
        (v.camera.yaw, v.camera.pitch) = (yaw.to_radians(), pitch.to_radians());
        v.label = Some(label);
        v
    }

    /// Вписать сцену заново при следующем показе.
    pub fn refit(&mut self) {
        self.fitted = false;
    }

    /// Ждёт ли окно вписывания (нажали «Показать всё»).
    pub fn needs_fit(&self) -> bool {
        !self.fitted
    }

    /// Камера под углами (градусы): поворот вокруг вертикали и наклон.
    pub fn set_view(&mut self, yaw: f32, pitch: f32) {
        (self.camera.yaw, self.camera.pitch) = (yaw.to_radians(), pitch.to_radians());
    }

    /// Панель видов и режимов над окном модели — иконки, как в cadcraft.
    pub fn toolbar(&mut self, ui: &mut egui::Ui) {
        use crate::icons::{Icon, button};
        const S: f32 = 26.0;
        let views = [(Icon::ViewFront, "Вид спереди", 0.0, 0.0), (Icon::ViewLeft, "Вид слева", 90.0, 0.0), (Icon::ViewTop, "Вид сверху", 0.0, 89.9), (Icon::ViewIso, "Аксонометрия (объёмный вид)", Camera::AXON.0, Camera::AXON.1)];
        ui.weak(crate::i18n::t("Вид"));
        for (icon, tip, yaw, pitch) in views {
            let on = (self.camera.yaw - f32::to_radians(yaw)).abs() < 1e-3 && (self.camera.pitch - f32::to_radians(pitch)).abs() < 1e-3;
            if button(ui, icon, S, tip, on).clicked() {
                (self.camera.yaw, self.camera.pitch) = (f32::to_radians(yaw), f32::to_radians(pitch));
            }
        }
        if button(ui, Icon::Fit, S, "Показать всё: вписать модель в окно", false).clicked() {
            self.fitted = false;
        }
        ui.separator();
        ui.weak(crate::i18n::t("Показ"));
        let icons = [Icon::ModeSolid, Icon::ModeHidden, Icon::ModeWire, Icon::ModeXRay, Icon::ModeTextured];
        let tips = [
            "Без невидимых: детали сплошные, видны только ближние рёбра",
            "Невидимые тонкие: как сплошной, плюс скрытые рёбра светлыми линиями",
            "Каркас: только рёбра деталей, насквозь",
            "Прозрачность (рентген): детали полупрозрачные, видны оси каждой детали",
            "В текстурах: материалы картинками (Балтламинат по артикулу)",
        ];
        for (((mode, _), icon), tip) in Mode::ALL.into_iter().zip(icons).zip(tips) {
            if button(ui, icon, S, tip, self.mode == mode).clicked() {
                self.mode = mode;
            }
        }
    }

    /// Деталь под точкой экрана (смещение от центра окна в точках egui).
    /// Ближайшее под точкой экрана: деталь (`Ok`) или фурнитура (`Err` — номер объекта).
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

    /// Окно модели. `revision` меняется, когда меняются детали — тогда сетка строится заново.
    /// Возвращает щелчок (по детали, по пустому месту, правой кнопкой) и отклик окна — к нему цепляется меню.
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
        if self.selected.as_deref() != Some(selected) || self.furniture_built != self.furniture || self.textures_built != self.textures || self.mode_built.map(|m| m == Mode::Textured) != Some(textured) {
            let tex = textured.then_some(&self.textures[..]);
            self.scene = Some(Scene::new(&rs.device, parts, &self.mesh, selected, &self.furniture, tex));
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
        // ЛКМ — вращение, ПКМ — сдвиг, колесо — масштаб относительно точки под курсором.
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
        if resp.dragged_by(egui::PointerButton::Primary) {
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
                // Точка под курсором остаётся на месте: сдвиг на долю смещения от центра.
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
        let Some(target) = &gpu.target else { return (click, Some(resp)) };
        let mut enc = rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("3d") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("3d"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.msaa,
                    depth_slice: None,
                    resolve_target: Some(&target.resolve),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.016, g: 0.0212, b: 0.0284, a: 1.0 }), store: wgpu::StoreOp::Discard },
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
            let grid = holes.end..holes.end + scene.grid_count;
            // Сетка пола — первой и с глубиной: детали её закрывают.
            pass.set_pipeline(&gpu.lines_visible);
            pass.set_vertex_buffer(0, scene.lines.slice(..));
            pass.draw(grid, 0..1);
            match self.mode {
                Mode::Solid | Mode::Hidden => {
                    pass.set_pipeline(&gpu.solid);
                    pass.set_vertex_buffer(0, scene.tris.slice(..));
                    pass.set_index_buffer(scene.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..scene.index_count, 0, 0..1);
                    pass.set_vertex_buffer(0, scene.lines.slice(..));
                    if self.mode == Mode::Hidden {
                        // shortcut: скрытые рёбра — светлой сплошной линией, не штрихом.
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
        if let Some(l) = self.label {
            ui.painter().text(rect.left_top() + Vec2::new(6.0, 4.0), egui::Align2::LEFT_TOP, l, egui::FontId::proportional(13.0), Color32::from_gray(90));
            ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(1.0, Color32::from_gray(150)), egui::StrokeKind::Inside);
        }
        if !self.errors.is_empty() {
            let text = format!("Не построены контуры деталей ({}): {}", self.errors.len(), self.errors.iter().map(|(n, e)| format!("«{n}» — {e}")).collect::<Vec<_>>().join("; "));
            ui.painter().text(rect.left_bottom() + Vec2::new(8.0, -8.0), egui::Align2::LEFT_BOTTOM, text, egui::FontId::proportional(13.0), Color32::from_rgb(200, 40, 40));
        }
        (click, Some(resp))
    }
}
