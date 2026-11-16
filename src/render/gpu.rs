use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

use super::{InstanceRaw, SceneRequest};
use crate::game::mesh::{Mesh, MeshLib, Primitive};
use crate::game::{LightKind, Shading};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
const MAX_LIGHTS: usize = 8;
const MAX_DIM: u32 = 4096;
/// نرمال‌ها فقط بین وجه‌هایی که زاویه‌شان کمتر از ~57° است نرم می‌شوند
const CREASE_COS: f32 = 0.55;

// =====================================================================
//  GPU data layouts (must match shader.wgsl)
// =====================================================================

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    nrm: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LightRaw {
    pos_kind: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlobalsRaw {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    sky_top: [f32; 4],
    sky_bottom: [f32; 4],
    ambient: [f32; 4],
    wire_color: [f32; 4],
    params: [f32; 4],
    counts: [u32; 4],
    lights: [LightRaw; MAX_LIGHTS],
}

const VERT_ATTRS: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
const INST_ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
    2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4
];

fn vertex_layouts() -> [wgpu::VertexBufferLayout<'static>; 2] {
    [
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERT_ATTRS,
        },
        wgpu::VertexBufferLayout {
            array_stride: size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INST_ATTRS,
        },
    ]
}

fn lin(c: f32) -> f32 {
    c.powf(2.2)
}

fn srgb(r: f32, g: f32, b: f32, a: f32) -> [f32; 4] {
    [lin(r / 255.0), lin(g / 255.0), lin(b / 255.0), a]
}

// =====================================================================
//  Meshes on the GPU
// =====================================================================

struct GpuMesh {
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    tris: u32,
    lib: wgpu::Buffer,
    lines: u32,
    double_sided: bool,
    rev: u64,
    last_used: u64,
}

fn newell(v: &[Vec3], f: &[usize]) -> Vec3 {
    let mut n = Vec3::ZERO;
    for i in 0..f.len() {
        let a = v[f[i]];
        let b = v[f[(i + 1) % f.len()]];
        n += Vec3::new(
            (a.y - b.y) * (a.z + b.z),
            (a.z - b.z) * (a.x + b.x),
            (a.x - b.x) * (a.y + b.y),
        );
    }
    n
}

/// مش چندضلعی را به مثلث، نرمال و خطوط یال تبدیل می‌کند.
/// هر گوشه‌ی هر وجه یک رأس مستقل است (نرمال‌ها با زاویه‌ی Crease نرم می‌شوند).
fn tessellate(m: &Mesh) -> (Vec<Vertex>, Vec<u32>, Vec<u32>) {
    let nv = m.verts.len();
    let nf = m.faces.len();

    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in &m.verts {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    let center = if nv > 0 { (lo + hi) * 0.5 } else { Vec3::ZERO };

    let valid = |f: &Vec<usize>| f.len() >= 3 && f.iter().all(|&i| i < nv);

    // نرمال هر وجه (وزن‌دار با مساحت) و جهت‌دهی به بیرون برای مش‌های بدون winding معتبر
    let mut fnorm: Vec<Vec3> = Vec::with_capacity(nf);
    let mut flip: Vec<bool> = Vec::with_capacity(nf);
    for f in &m.faces {
        if !valid(f) {
            fnorm.push(Vec3::ZERO);
            flip.push(false);
            continue;
        }
        let n = newell(&m.verts, f);
        let mut fl = false;
        if !m.oriented && !m.double_sided {
            let c = f.iter().map(|&i| m.verts[i]).sum::<Vec3>() / f.len() as f32;
            fl = n.dot(c - center) < 0.0;
        }
        fnorm.push(if fl { -n } else { n });
        flip.push(fl);
    }

    // فهرست وجه‌های متصل به هر رأس (CSR)
    let smooth = nf <= 60_000;
    let mut start: Vec<u32> = Vec::new();
    let mut list: Vec<u32> = Vec::new();
    if smooth {
        start = vec![0; nv + 1];
        for f in &m.faces {
            if valid(f) {
                for &v in f {
                    start[v + 1] += 1;
                }
            }
        }
        for i in 0..nv {
            start[i + 1] += start[i];
        }
        let mut fill = start.clone();
        list = vec![0; start[nv] as usize];
        for (fi, f) in m.faces.iter().enumerate() {
            if valid(f) {
                for &v in f {
                    list[fill[v] as usize] = fi as u32;
                    fill[v] += 1;
                }
            }
        }
    }

    let corners: usize = m.faces.iter().map(|f| f.len()).sum();
    let mut verts: Vec<Vertex> = Vec::with_capacity(corners);
    let mut tris: Vec<u32> = Vec::with_capacity(corners * 2);
    let mut lines: Vec<u32> = Vec::with_capacity(corners * 2);
    let mut seen: HashSet<(u32, u32)> = HashSet::with_capacity(corners);

    for (fi, f) in m.faces.iter().enumerate() {
        let fnn = fnorm[fi].normalize_or_zero();
        if !valid(f) || fnn == Vec3::ZERO {
            continue;
        }
        let base = verts.len() as u32;
        for &vi in f {
            let n = if smooth {
                let mut acc = Vec3::ZERO;
                for &fj in &list[start[vi] as usize..start[vi + 1] as usize] {
                    let nj = fnorm[fj as usize];
                    let l = nj.length();
                    if l > 1e-12 && nj.dot(fnn) / l > CREASE_COS {
                        acc += nj;
                    }
                }
                let a = acc.normalize_or_zero();
                if a == Vec3::ZERO { fnn } else { a }
            } else {
                fnn
            };
            verts.push(Vertex { pos: m.verts[vi].to_array(), nrm: n.to_array() });
        }
        let k = f.len() as u32;
        for i in 1..k - 1 {
            if flip[fi] {
                tris.extend_from_slice(&[base, base + i + 1, base + i]);
            } else {
                tris.extend_from_slice(&[base, base + i, base + i + 1]);
            }
        }
        for i in 0..k {
            let a = f[i as usize] as u32;
            let b = f[((i + 1) % k) as usize] as u32;
            if seen.insert((a.min(b), a.max(b))) {
                lines.push(base + i);
                lines.push(base + (i + 1) % k);
            }
        }
    }
    (verts, tris, lines)
}

fn build_gpu_mesh(device: &wgpu::Device, m: &Mesh, frame: u64) -> GpuMesh {
    let (mut verts, mut tris, mut lines) = tessellate(m);
    let (tri_n, line_n) = (tris.len() as u32 / 3, lines.len() as u32 / 2);
    // بافر خالی ممنوع نیست، ولی این‌طور امن‌تر است
    if verts.is_empty() {
        verts.push(Vertex::zeroed());
    }
    if tris.is_empty() {
        tris.extend_from_slice(&[0, 0, 0]);
    }
    if lines.is_empty() {
        lines.extend_from_slice(&[0, 0]);
    }
    let mk = |label: &str, bytes: &[u8], usage: wgpu::BufferUsages| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytes,
            usage,
        })
    };
    GpuMesh {
        vb: mk("mesh vb", bytemuck::cast_slice(&verts), wgpu::BufferUsages::VERTEX),
        ib: mk("mesh ib", bytemuck::cast_slice(&tris), wgpu::BufferUsages::INDEX),
        lib: mk("mesh lines", bytemuck::cast_slice(&lines), wgpu::BufferUsages::INDEX),
        tris: tri_n,
        lines: line_n,
        double_sided: m.double_sided,
        rev: m.rev,
        last_used: frame,
    }
}

// =====================================================================
//  Render targets
// =====================================================================

struct Targets {
    size: [u32; 2],
    samples: u32,
    _color: wgpu::Texture,
    color_view: wgpu::TextureView,
    _msaa: Option<wgpu::Texture>,
    msaa_view: Option<wgpu::TextureView>,
    _depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    _mask: wgpu::Texture,
    mask_view: wgpu::TextureView,
    outline_bg: wgpu::BindGroup,
}

fn make_targets(
    device: &wgpu::Device,
    outline_layout: &wgpu::BindGroupLayout,
    size: [u32; 2],
    samples: u32,
) -> Targets {
    let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
    let tex = |label: &str, format: wgpu::TextureFormat, usage: wgpu::TextureUsages, count: u32| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: extent,
            mip_level_count: 1,
            sample_count: count,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let rt_tex = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;

    let color = tex("scene color", COLOR_FORMAT, rt_tex, 1);
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());

    let (msaa, msaa_view) = if samples > 1 {
        let t = tex("scene msaa", COLOR_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT, samples);
        let v = t.create_view(&wgpu::TextureViewDescriptor::default());
        (Some(t), Some(v))
    } else {
        (None, None)
    };

    let depth = tex("scene depth", DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT, samples);
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

    let mask = tex("selection mask", MASK_FORMAT, rt_tex, 1);
    let mask_view = mask.create_view(&wgpu::TextureViewDescriptor::default());

    let outline_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("outline bg"),
        layout: outline_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&mask_view),
        }],
    });

    Targets {
        size,
        samples,
        _color: color,
        color_view,
        _msaa: msaa,
        msaa_view,
        _depth: depth,
        depth_view,
        _mask: mask,
        mask_view,
        outline_bg,
    }
}

// =====================================================================
//  Pipelines
// =====================================================================

struct Pipelines {
    sky: wgpu::RenderPipeline,
    mesh: wgpu::RenderPipeline,
    mesh_2s: wgpu::RenderPipeline,
    wire: wgpu::RenderPipeline,
    grid: wgpu::RenderPipeline,
}

fn depth_state(write: bool, compare: wgpu::CompareFunction) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: write,
        depth_compare: compare,
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

fn target(format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>) -> Option<wgpu::ColorTargetState> {
    Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })
}

fn make_pipelines(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    samples: u32,
) -> Pipelines {
    let ms = wgpu::MultisampleState { count: samples, mask: !0, alpha_to_coverage_enabled: false };
    let vbufs = vertex_layouts();

    let mesh = |label: &str, cull: Option<wgpu::Face>| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState { module, entry_point: "vs_mesh", buffers: &vbufs },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: cull,
                ..Default::default()
            },
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Greater)),
            multisample: ms,
            fragment: Some(wgpu::FragmentState {
                module,
                entry_point: "fs_mesh",
                targets: &[target(COLOR_FORMAT, None)],
            }),
            multiview: None,
        })
    };

    let sky = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sky"),
        layout: Some(layout),
        vertex: wgpu::VertexState { module, entry_point: "vs_full", buffers: &[] },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(depth_state(false, wgpu::CompareFunction::Always)),
        multisample: ms,
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: "fs_sky",
            targets: &[target(COLOR_FORMAT, None)],
        }),
        multiview: None,
    });

    let wire = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("wire"),
        layout: Some(layout),
        vertex: wgpu::VertexState { module, entry_point: "vs_wire", buffers: &vbufs },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(depth_state(false, wgpu::CompareFunction::Greater)),
        multisample: ms,
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: "fs_wire",
            targets: &[target(COLOR_FORMAT, Some(wgpu::BlendState::ALPHA_BLENDING))],
        }),
        multiview: None,
    });

    let grid = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("grid"),
        layout: Some(layout),
        vertex: wgpu::VertexState { module, entry_point: "vs_full", buffers: &[] },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(depth_state(false, wgpu::CompareFunction::Greater)),
        multisample: ms,
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: "fs_grid",
            targets: &[target(COLOR_FORMAT, Some(wgpu::BlendState::ALPHA_BLENDING))],
        }),
        multiview: None,
    });

    Pipelines {
        sky,
        mesh: mesh("mesh", Some(wgpu::Face::Back)),
        mesh_2s: mesh("mesh double sided", None),
        wire,
        grid,
    }
}

// =====================================================================
//  SceneRenderer
// =====================================================================

pub struct RenderResult {
    /// true یعنی Texture عوض شده و باید دوباره به egui وصل شود
    pub resized: bool,
    pub tris: u32,
    pub draw_calls: u32,
}

pub struct SceneRenderer {
    module: wgpu::ShaderModule,
    outline_layout: wgpu::BindGroupLayout,
    scene_layout: wgpu::PipelineLayout,
    globals_buf: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    pipes: Pipelines,
    mask_pipe: wgpu::RenderPipeline,
    outline_pipe: wgpu::RenderPipeline,
    samples: u32,
    targets: Targets,
    meshes: HashMap<Primitive, GpuMesh>,
    inst_buf: wgpu::Buffer,
    inst_cap: usize,
    frame: u64,
    last_hash: u64,
    has_frame: bool,
}

fn make_inst_buf(device: &wgpu::Device, cap: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("instances"),
        size: (cap * size_of::<InstanceRaw>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl SceneRenderer {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let outline_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("outline layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene pipeline layout"),
            bind_group_layouts: &[&globals_layout],
            push_constant_ranges: &[],
        });
        let outline_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("outline pipeline layout"),
            bind_group_layouts: &[&globals_layout, &outline_layout],
            push_constant_ranges: &[],
        });

        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: size_of::<GlobalsRaw>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals bg"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals_buf.as_entire_binding() }],
        });

        let samples = 4;
        let pipes = make_pipelines(device, &module, &scene_layout, samples);
        let vbufs = vertex_layouts();

        let mask_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mask"),
            layout: Some(&scene_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: "vs_mask", buffers: &vbufs },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: "fs_mask",
                targets: &[target(MASK_FORMAT, None)],
            }),
            multiview: None,
        });
        let outline_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("outline"),
            layout: Some(&outline_pl),
            vertex: wgpu::VertexState { module: &module, entry_point: "vs_full", buffers: &[] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: "fs_outline",
                targets: &[target(COLOR_FORMAT, Some(wgpu::BlendState::ALPHA_BLENDING))],
            }),
            multiview: None,
        });

        let targets = make_targets(device, &outline_layout, [16, 16], samples);
        let inst_cap = 256;
        Self {
            module,
            outline_layout,
            scene_layout,
            globals_buf,
            globals_bg,
            pipes,
            mask_pipe,
            outline_pipe,
            samples,
            targets,
            meshes: HashMap::new(),
            inst_buf: make_inst_buf(device, inst_cap),
            inst_cap,
            frame: 0,
            last_hash: 0,
            has_frame: false,
        }
    }

    /// Texture نهایی که egui نمایش می‌دهد
    pub fn color_view(&self) -> &wgpu::TextureView {
        &self.targets.color_view
    }

    fn ensure_mesh(&mut self, device: &wgpu::Device, lib: &MeshLib, prim: Primitive) {
        let m = lib.get(prim);
        if let Some(g) = self.meshes.get_mut(&prim) {
            if g.rev == m.rev {
                g.last_used = self.frame;
                return;
            }
        }
        let g = build_gpu_mesh(device, m, self.frame);
        self.meshes.insert(prim, g);
    }

    fn globals(req: &SceneRequest) -> GlobalsRaw {
        let vp = req.view_proj;
        let mut lights = [LightRaw::zeroed(); MAX_LIGHTS];
        let n = req.lights.len().min(MAX_LIGHTS);
        for (i, l) in req.lights.iter().take(n).enumerate() {
            let kind = match l.kind {
                LightKind::Directional => 0.0,
                LightKind::Point => 1.0,
            };
            lights[i] = LightRaw {
                pos_kind: [l.vec.x, l.vec.y, l.vec.z, kind],
                color: [l.color.x, l.color.y, l.color.z, 0.0],
            };
        }
        let wire = if req.shading == Shading::Wire {
            srgb(170.0, 195.0, 230.0, 1.0)
        } else {
            [0.0, 0.0, 0.0, 0.42]
        };
        GlobalsRaw {
            view_proj: vp.to_cols_array_2d(),
            inv_view_proj: vp.inverse().to_cols_array_2d(),
            eye: [req.eye.x, req.eye.y, req.eye.z, 1.0],
            sky_top: srgb(58.0, 66.0, 82.0, 1.0),
            sky_bottom: srgb(28.0, 31.0, 38.0, 1.0),
            ambient: [0.20, 0.22, 0.26, 1.0],
            wire_color: wire,
            params: [req.px[0] as f32, req.px[1] as f32, 5.0e-5, 0.0],
            counts: [n as u32, 0, 0, 0],
            lights,
        }
    }

    fn hash_request(&self, req: &SceneRequest, g: &GlobalsRaw) -> u64 {
        let mut h = DefaultHasher::new();
        bytemuck::bytes_of(g).hash(&mut h);
        bytemuck::cast_slice::<InstanceRaw, u8>(&req.instances).hash(&mut h);
        req.px.hash(&mut h);
        req.samples.hash(&mut h);
        (req.shading as u8).hash(&mut h);
        req.grid.hash(&mut h);
        for b in &req.batches {
            b.prim.hash(&mut h);
            b.start.hash(&mut h);
            b.count.hash(&mut h);
            if let Some(g) = self.meshes.get(&b.prim) {
                g.rev.hash(&mut h);
            }
        }
        if let Some((p, inst)) = &req.selected {
            p.hash(&mut h);
            bytemuck::bytes_of(inst).hash(&mut h);
            if let Some(g) = self.meshes.get(p) {
                g.rev.hash(&mut h);
            }
        }
        h.finish()
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        req: &SceneRequest,
        lib: &MeshLib,
    ) -> RenderResult {
        self.frame += 1;
        if self.frame % 300 == 0 {
            let f = self.frame;
            self.meshes
                .retain(|p, g| !matches!(p, Primitive::Custom(_)) || f - g.last_used < 900);
        }

        let samples = if req.samples >= 4 { 4 } else { 1 };
        let size = [req.px[0].clamp(1, MAX_DIM), req.px[1].clamp(1, MAX_DIM)];
        let mut resized = false;

        if samples != self.samples {
            self.samples = samples;
            self.pipes = make_pipelines(device, &self.module, &self.scene_layout, samples);
        }
        if self.targets.size != size || self.targets.samples != samples {
            self.targets = make_targets(device, &self.outline_layout, size, samples);
            resized = true;
        }

        for b in &req.batches {
            self.ensure_mesh(device, lib, b.prim);
        }
        if let Some((p, _)) = &req.selected {
            self.ensure_mesh(device, lib, *p);
        }

        // آمار
        let mut tris = 0u32;
        let mut calls = 1u32;
        for b in &req.batches {
            if let Some(g) = self.meshes.get(&b.prim) {
                if req.shading != Shading::Wire {
                    tris += g.tris * b.count;
                    calls += 1;
                }
                if req.shading != Shading::Solid {
                    calls += 1;
                }
            }
        }
        calls += req.grid as u32;
        if req.selected.is_some() {
            calls += 2;
        }

        let globals = Self::globals(req);
        let hash = self.hash_request(req, &globals);
        if !resized && self.has_frame && hash == self.last_hash {
            return RenderResult { resized: false, tris, draw_calls: calls };
        }
        self.last_hash = hash;
        self.has_frame = true;

        // آپلود داده‌ها
        queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
        let n = req.instances.len();
        let need = n + 1;
        if need > self.inst_cap {
            self.inst_cap = need.next_power_of_two().max(256);
            self.inst_buf = make_inst_buf(device, self.inst_cap);
        }
        if n > 0 {
            queue.write_buffer(&self.inst_buf, 0, bytemuck::cast_slice(&req.instances));
        }
        if let Some((_, inst)) = &req.selected {
            let off = (n * size_of::<InstanceRaw>()) as u64;
            queue.write_buffer(&self.inst_buf, off, bytemuck::bytes_of(inst));
        }

        // ---- از اینجا به بعد فقط خواندن ----
        let this: &SceneRenderer = &*self;
        let t = &this.targets;
        let msaa = t.msaa_view.is_some();

        {
            let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: t.msaa_view.as_ref().unwrap_or(&t.color_view),
                    resolve_target: t.msaa_view.as_ref().map(|_| &t.color_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: if msaa { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rp.set_bind_group(0, &this.globals_bg, &[]);

            rp.set_pipeline(&this.pipes.sky);
            rp.draw(0..3, 0..1);

            if req.shading != Shading::Wire {
                let mut cur: Option<bool> = None;
                for b in &req.batches {
                    let Some(gm) = this.meshes.get(&b.prim) else { continue };
                    if gm.tris == 0 {
                        continue;
                    }
                    if cur != Some(gm.double_sided) {
                        rp.set_pipeline(if gm.double_sided { &this.pipes.mesh_2s } else { &this.pipes.mesh });
                        cur = Some(gm.double_sided);
                    }
                    rp.set_vertex_buffer(0, gm.vb.slice(..));
                    rp.set_vertex_buffer(1, this.inst_buf.slice(..));
                    rp.set_index_buffer(gm.ib.slice(..), wgpu::IndexFormat::Uint32);
                    rp.draw_indexed(0..gm.tris * 3, 0, b.start..b.start + b.count);
                }
            }

            if req.shading != Shading::Solid {
                rp.set_pipeline(&this.pipes.wire);
                for b in &req.batches {
                    let Some(gm) = this.meshes.get(&b.prim) else { continue };
                    if gm.lines == 0 {
                        continue;
                    }
                    rp.set_vertex_buffer(0, gm.vb.slice(..));
                    rp.set_vertex_buffer(1, this.inst_buf.slice(..));
                    rp.set_index_buffer(gm.lib.slice(..), wgpu::IndexFormat::Uint32);
                    rp.draw_indexed(0..gm.lines * 2, 0, b.start..b.start + b.count);
                }
            }

            if req.grid {
                rp.set_pipeline(&this.pipes.grid);
                rp.draw(0..3, 0..1);
            }
        }

        // Outline آبجکت انتخاب‌شده: Mask سپس Edge Detect
        if let Some((prim, _)) = &req.selected {
            if let Some(gm) = this.meshes.get(prim) {
                if gm.tris > 0 {
                    let idx = n as u32;
                    {
                        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("mask pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &t.mask_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        rp.set_pipeline(&this.mask_pipe);
                        rp.set_bind_group(0, &this.globals_bg, &[]);
                        rp.set_vertex_buffer(0, gm.vb.slice(..));
                        rp.set_vertex_buffer(1, this.inst_buf.slice(..));
                        rp.set_index_buffer(gm.ib.slice(..), wgpu::IndexFormat::Uint32);
                        rp.draw_indexed(0..gm.tris * 3, 0, idx..idx + 1);
                    }
                    {
                        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("outline pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &t.color_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                        rp.set_pipeline(&this.outline_pipe);
                        rp.set_bind_group(0, &this.globals_bg, &[]);
                        rp.set_bind_group(1, &t.outline_bg, &[]);
                        rp.draw(0..3, 0..1);
                    }
                }
            }
        }

        RenderResult { resized, tris, draw_calls: calls }
    }
}