use glam::Vec3;
use std::collections::{BTreeSet, HashMap};
use std::f32::consts::PI;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Primitive {
    Cube,
    Sphere,
    Cylinder,
    Pyramid,
    Plane,
    Custom(u32),
}

impl Primitive {
    pub const ALL: [Primitive; 5] = [
        Primitive::Cube,
        Primitive::Sphere,
        Primitive::Cylinder,
        Primitive::Pyramid,
        Primitive::Plane,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Primitive::Cube => "Cube",
            Primitive::Sphere => "Sphere",
            Primitive::Cylinder => "Cylinder",
            Primitive::Pyramid => "Pyramid",
            Primitive::Plane => "Plane",
            Primitive::Custom(_) => "Custom Mesh",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Primitive::Cube => "📦",
            Primitive::Sphere => "●",
            Primitive::Cylinder => "▮",
            Primitive::Pyramid => "▲",
            Primitive::Plane => "▬",
            Primitive::Custom(_) => "◆",
        }
    }

    pub fn default_color(self) -> [f32; 3] {
        match self {
            Primitive::Cube => [0.30, 0.55, 0.95],
            Primitive::Sphere => [0.92, 0.35, 0.30],
            Primitive::Cylinder => [0.35, 0.80, 0.50],
            Primitive::Pyramid => [0.95, 0.75, 0.25],
            Primitive::Plane => [0.55, 0.57, 0.60],
            Primitive::Custom(_) => [0.75, 0.75, 0.78],
        }
    }
}

pub struct Mesh {
    pub verts: Vec<Vec3>,
    pub faces: Vec<Vec<usize>>,
    pub edges: Vec<(usize, usize)>,
    pub double_sided: bool,
    /// true یعنی جهت وجه‌ها (winding) قابل‌اعتماد است
    pub oriented: bool,
    /// با هر تغییر مش سفارشی زیاد می‌شود (کلید کش GPU)
    pub rev: u64,
    /// شعاع کره‌ی محیطی حول مبدأ محلی (برای Frustum Culling)
    pub radius: f32,
}

impl Mesh {
    fn build(verts: Vec<Vec3>, faces: Vec<Vec<usize>>, double_sided: bool) -> Self {
        let mut set = BTreeSet::new();
        for f in &faces {
            for i in 0..f.len() {
                let a = f[i];
                let b = f[(i + 1) % f.len()];
                set.insert((a.min(b), a.max(b)));
            }
        }
        let radius = verts.iter().map(|v| v.length()).fold(0.0_f32, f32::max);
        Self {
            verts,
            faces,
            edges: set.into_iter().collect(),
            double_sided,
            oriented: false,
            rev: 0,
            radius,
        }
    }
}

pub struct MeshLib {
    map: HashMap<Primitive, Mesh>,
    next_rev: u64,
}

impl MeshLib {
    pub fn new() -> Self {
        let mut map = HashMap::new();
        map.insert(Primitive::Cube, cube());
        map.insert(Primitive::Sphere, sphere(10, 18));
        map.insert(Primitive::Cylinder, cylinder(20));
        map.insert(Primitive::Pyramid, pyramid());
        map.insert(Primitive::Plane, plane());
        Self { map, next_rev: 0 }
    }

    pub fn get(&self, p: Primitive) -> &Mesh {
        self.map.get(&p).unwrap_or(&self.map[&Primitive::Cube])
    }

    pub fn set_custom(&mut self, id: u32, verts: Vec<Vec3>, mut faces: Vec<Vec<usize>>, double_sided: bool) {
        faces.retain(|f| f.len() >= 3 && f.iter().all(|&i| i < verts.len()));
        let mut m = Mesh::build(verts, faces, double_sided);
        m.oriented = true;
        self.next_rev += 1;
        m.rev = self.next_rev;
        self.map.insert(Primitive::Custom(id), m);
    }
}

fn cube() -> Mesh {
    let h = 0.5;
    let v = vec![
        Vec3::new(-h, -h, -h),
        Vec3::new(h, -h, -h),
        Vec3::new(h, h, -h),
        Vec3::new(-h, h, -h),
        Vec3::new(-h, -h, h),
        Vec3::new(h, -h, h),
        Vec3::new(h, h, h),
        Vec3::new(-h, h, h),
    ];
    let f = vec![
        vec![0, 1, 2, 3],
        vec![5, 4, 7, 6],
        vec![4, 0, 3, 7],
        vec![1, 5, 6, 2],
        vec![3, 2, 6, 7],
        vec![4, 5, 1, 0],
    ];
    Mesh::build(v, f, false)
}

fn pyramid() -> Mesh {
    let v = vec![
        Vec3::new(-0.5, -0.5, -0.5),
        Vec3::new(0.5, -0.5, -0.5),
        Vec3::new(0.5, -0.5, 0.5),
        Vec3::new(-0.5, -0.5, 0.5),
        Vec3::new(0.0, 0.5, 0.0),
    ];
    let f = vec![
        vec![0, 1, 2, 3],
        vec![0, 1, 4],
        vec![1, 2, 4],
        vec![2, 3, 4],
        vec![3, 0, 4],
    ];
    Mesh::build(v, f, false)
}

fn plane() -> Mesh {
    let v = vec![
        Vec3::new(-1.0, 0.0, -1.0),
        Vec3::new(1.0, 0.0, -1.0),
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::new(-1.0, 0.0, 1.0),
    ];
    Mesh::build(v, vec![vec![0, 1, 2, 3]], true)
}

fn cylinder(n: usize) -> Mesh {
    let mut v = Vec::new();
    for i in 0..n {
        let t = 2.0 * PI * i as f32 / n as f32;
        v.push(Vec3::new(0.5 * t.cos(), -0.5, 0.5 * t.sin()));
    }
    for i in 0..n {
        let t = 2.0 * PI * i as f32 / n as f32;
        v.push(Vec3::new(0.5 * t.cos(), 0.5, 0.5 * t.sin()));
    }
    let mut f = Vec::new();
    for i in 0..n {
        let j = (i + 1) % n;
        f.push(vec![i, j, n + j, n + i]);
    }
    f.push((0..n).collect());
    f.push((n..2 * n).collect());
    Mesh::build(v, f, false)
}

fn sphere(stacks: usize, slices: usize) -> Mesh {
    let mut v = vec![Vec3::new(0.0, 0.5, 0.0)];
    for i in 1..stacks {
        let phi = PI * i as f32 / stacks as f32;
        let y = 0.5 * phi.cos();
        let r = 0.5 * phi.sin();
        for j in 0..slices {
            let t = 2.0 * PI * j as f32 / slices as f32;
            v.push(Vec3::new(r * t.cos(), y, r * t.sin()));
        }
    }
    v.push(Vec3::new(0.0, -0.5, 0.0));

    let ring = |i: usize, j: usize| 1 + (i - 1) * slices + (j % slices);
    let bottom = v.len() - 1;
    let mut f = Vec::new();
    for j in 0..slices {
        f.push(vec![0, ring(1, j + 1), ring(1, j)]);
    }
    for i in 1..stacks - 1 {
        for j in 0..slices {
            f.push(vec![ring(i, j), ring(i, j + 1), ring(i + 1, j + 1), ring(i + 1, j)]);
        }
    }
    for j in 0..slices {
        f.push(vec![bottom, ring(stacks - 1, j), ring(stacks - 1, j + 1)]);
    }
    Mesh::build(v, f, false)
}