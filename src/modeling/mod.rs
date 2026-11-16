pub mod mesh;
pub mod ops;

use std::fs;
use std::path::Path;

use glam::{Mat4, Quat, Vec2, Vec3};
use mesh::{EditMesh, NONE};

use crate::game::mesh::Primitive;
use crate::game::{Component, EntityId, GameState, OrbitCam, PlayMode};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SelMode {
    Vertex,
    Edge,
    Face,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Constraint {
    Free,
    X,
    Y,
    Z,
    Normal,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModalKind {
    Grab,
    Rotate,
    Scale,
    Inset,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Grab,
    Rotate,
    Scale,
    Extrude,
    Inset,
    Duplicate,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PrimKind {
    Cube,
    Grid,
    Sphere,
    Cylinder,
    Cone,
    Torus,
}

// =====================================================================
//  Camera math
// =====================================================================

#[derive(Clone, Copy)]
pub struct Cam {
    pub view: Mat4,
    pub proj: Mat4,
    pub eye: Vec3,
    pub min: Vec2,
    pub size: Vec2,
    pub fov: f32,
    /// نیم‌ارتفاع دید در حالت Ortho؛ صفر یعنی Perspective
    pub ortho_h: f32,
}

const NEAR: f32 = -0.05;

impl Cam {
    pub fn new(view: Mat4, proj: Mat4, eye: Vec3, min: Vec2, size: Vec2, fov: f32, ortho_h: f32) -> Self {
        Self { view, proj, eye, min, size, fov, ortho_h }
    }

    pub fn is_ortho(&self) -> bool {
        self.ortho_h > 0.0
    }

    fn project_view(&self, v: Vec3) -> Vec2 {
        let c = self.proj * v.extend(1.0);
        let n = c.truncate() / c.w;
        Vec2::new(
            self.min.x + (n.x * 0.5 + 0.5) * self.size.x,
            self.min.y + (0.5 - n.y * 0.5) * self.size.y,
        )
    }

    pub fn project(&self, p: Vec3) -> Option<Vec2> {
        let v = (self.view * p.extend(1.0)).truncate();
        if v.z > NEAR {
            return None;
        }
        Some(self.project_view(v))
    }

    pub fn segment(&self, a: Vec3, b: Vec3) -> Option<[Vec2; 2]> {
        let mut va = (self.view * a.extend(1.0)).truncate();
        let mut vb = (self.view * b.extend(1.0)).truncate();
        if va.z > NEAR && vb.z > NEAR {
            return None;
        }
        if va.z > NEAR {
            let t = (NEAR - va.z) / (vb.z - va.z);
            va += (vb - va) * t;
        } else if vb.z > NEAR {
            let t = (NEAR - vb.z) / (va.z - vb.z);
            vb += (va - vb) * t;
        }
        Some([self.project_view(va), self.project_view(vb)])
    }

    pub fn right(&self) -> Vec3 {
        Vec3::new(self.view.x_axis.x, self.view.y_axis.x, self.view.z_axis.x)
    }
    pub fn up(&self) -> Vec3 {
        Vec3::new(self.view.x_axis.y, self.view.y_axis.y, self.view.z_axis.y)
    }
    pub fn forward(&self) -> Vec3 {
        -Vec3::new(self.view.x_axis.z, self.view.y_axis.z, self.view.z_axis.z)
    }

    /// بردار از نقطه به سمت چشم (در Ortho ثابت است)
    pub fn to_eye(&self, p: Vec3) -> Vec3 {
        if self.is_ortho() { -self.forward() } else { self.eye - p }
    }

    /// عمق در راستای دید (بزرگ‌تر = دورتر)
    pub fn depth(&self, p: Vec3) -> f32 {
        (p - self.eye).dot(self.forward())
    }

    pub fn units_per_px(&self, p: Vec3) -> f32 {
        if self.is_ortho() {
            return 2.0 * self.ortho_h / self.size.y.max(1.0);
        }
        let depth = self.depth(p).max(0.1);
        2.0 * depth * (self.fov.to_radians() * 0.5).tan() / self.size.y.max(1.0)
    }
}

pub fn dist_seg(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_squared();
    let t = if l2 < 1e-6 { 0.0 } else { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) };
    (p - (a + ab * t)).length()
}

fn point_in_poly(p: Vec2, poly: &[Vec2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// وزن Proportional Editing (smoothstep)
fn weight(d: f32, r: f32) -> f32 {
    if d <= 0.0 {
        return 1.0;
    }
    let t = (1.0 - d / r.max(1e-4)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// =====================================================================
//  Modal operator
// =====================================================================

pub struct Modal {
    pub kind: ModalKind,
    pub constraint: Constraint,
    pub start: Vec2,
    pub pivot: Vec3,
    pub normal: Vec3,
    /// (رأس، موقعیت اولیه، فاصله تا انتخاب؛ ۰ برای انتخاب‌شده‌ها)
    pub orig: Vec<(u32, Vec3, f32)>,
    pub inset: Vec<(u32, Vec3, Vec3)>,
    pub num: String,
}

// =====================================================================
//  ModelState
// =====================================================================

pub struct ModelState {
    pub mesh: EditMesh,
    pub mode: SelMode,
    pub cam: OrbitCam,
    pub ortho: bool,
    pub undo: Vec<EditMesh>,
    pub redo: Vec<EditMesh>,
    pub modal: Option<Modal>,
    pub cmd: Option<Cmd>,

    // نمایش
    pub solid: bool,
    pub material: bool,
    pub smooth_shade: bool,
    pub cull: bool,
    pub show_edges: bool,
    pub xray: bool,
    pub grid: bool,
    pub normals: bool,
    pub snap: bool,
    pub show_context: bool,

    // اتصال به آبجکت بازی
    pub target: Option<EntityId>,
    pub asset: Option<u32>,
    pub target_name: String,
    pub live: bool,
    pub color: [f32; 3],
    pub needs_sync: bool,

    // Modifier ها (غیرمخرب)
    pub mirror: [bool; 3],
    pub subsurf: u32,
    pub eval: Option<EditMesh>,

    // تنظیم ابزارها
    pub proportional: bool,
    pub prop_radius: f32,
    pub cuts: usize,
    pub weld_eps: f32,
    pub smooth_iter: u32,
    pub smooth_amt: f32,
    pub solidify_t: f32,
    pub sphere_f: f32,
    pub add_size: f32,
    pub grab_on_add: bool,
    pub cursor: Vec3,

    pub obj_path: String,
    pub status: String,
    pub log: Vec<String>,
    pub box_start: Option<Vec2>,

    // کش هر فریم
    pub open: bool,
    pub proj: Vec<Option<Vec2>>,
    pub front: Vec<bool>,
    pub vis_v: Vec<bool>,
    pub fnorm: Vec<Vec3>,
    pub fcen: Vec<Vec3>,
    pub vnorm: Vec<Vec3>,
}

impl ModelState {
    pub fn new() -> Self {
        let mut cam = OrbitCam::new();
        cam.target = Vec3::ZERO;
        cam.dist = 4.0;
        let mut m = mesh::cube();
        for v in m.verts.iter_mut() {
            *v *= 0.5;
        }
        Self {
            mesh: m,
            mode: SelMode::Face,
            cam,
            ortho: false,
            undo: Vec::new(),
            redo: Vec::new(),
            modal: None,
            cmd: None,
            solid: true,
            material: true,
            smooth_shade: false,
            cull: true,
            show_edges: true,
            xray: false,
            grid: true,
            normals: false,
            snap: false,
            show_context: true,
            target: None,
            asset: None,
            target_name: String::new(),
            live: true,
            color: [0.75, 0.75, 0.78],
            needs_sync: false,
            mirror: [false; 3],
            subsurf: 0,
            eval: None,
            proportional: false,
            prop_radius: 1.0,
            cuts: 1,
            weld_eps: 0.0001,
            smooth_iter: 2,
            smooth_amt: 0.5,
            solidify_t: 0.1,
            sphere_f: 1.0,
            add_size: 1.0,
            grab_on_add: true,
            cursor: Vec3::ZERO,
            obj_path: "models/model.obj".to_string(),
            status: String::new(),
            log: vec!["Modeling ready. In Game Engine select an object and press Tab to edit its mesh.".into()],
            box_start: None,
            open: false,
            proj: Vec::new(),
            front: Vec::new(),
            vis_v: Vec::new(),
            fnorm: Vec::new(),
            fcen: Vec::new(),
            vnorm: Vec::new(),
        }
    }

    pub fn log(&mut self, s: impl Into<String>) {
        if self.log.len() > 200 {
            self.log.drain(..50);
        }
        let s = s.into();
        self.status = s.clone();
        self.log.push(s);
    }

    // =================================================================
    //  اتصال به Game Engine
    // =================================================================

    /// مش یک آبجکت بازی را برای ویرایش باز می‌کند
    pub fn load_entity(&mut self, gs: &mut GameState, eid: EntityId) {
        if gs.mode != PlayMode::Edit {
            self.log("Stop Play mode before editing a mesh");
            return;
        }
        let Some(e) = gs.scene.get(eid) else { return };
        let name = e.name.clone();
        let existing = e.mesh();

        let (m, color, dbl) = match existing {
            Some((prim, color)) => {
                let src = gs.meshes.get(prim);
                let mut m = EditMesh {
                    verts: src.verts.clone(),
                    faces: src
                        .faces
                        .iter()
                        .map(|f| f.iter().map(|&i| i as u32).collect())
                        .collect(),
                    ..Default::default()
                };
                m.rebuild_edges();
                m.recalc_outside();
                (m, color, src.double_sided)
            }
            None => {
                let mut m = mesh::cube();
                for v in m.verts.iter_mut() {
                    *v *= 0.5;
                }
                (m, [0.75, 0.75, 0.78], false)
            }
        };

        // Single User: اگر مش با آبجکت دیگری مشترک است، کپی مستقل بساز
        let asset = match existing {
            Some((Primitive::Custom(id), _)) if gs.model_users(id) <= 1 => id,
            _ => gs.alloc_model_id(),
        };

        self.mesh = m;
        self.cull = !dbl;
        self.undo.clear();
        self.redo.clear();
        self.modal = None;
        self.target = Some(eid);
        self.asset = Some(asset);
        self.target_name = name.clone();
        self.color = color;
        self.mirror = [false; 3];
        self.subsurf = 0;
        self.eval = None;
        self.needs_sync = true;
        self.frame_all();
        self.log(format!("Editing '{name}' (live sync)"));
    }

    /// مش فعلی را به‌عنوان یک آبجکت جدید به صحنه اضافه می‌کند
    pub fn send_as_new(&mut self, gs: &mut GameState) {
        if gs.mode != PlayMode::Edit {
            self.log("Stop Play mode first");
            return;
        }
        if !self.rebuild_eval() {
            return;
        }
        let id = gs.alloc_model_id();
        self.upload(gs, id);
        let eid = gs.create(
            "Model",
            vec![Component::Mesh { prim: Primitive::Custom(id), color: self.color }],
            None,
        );
        self.target = Some(eid);
        self.asset = Some(id);
        self.target_name = "Model".into();
        self.log("Sent to scene as a new object");
    }

    pub fn detach(&mut self) {
        self.target = None;
        self.asset = None;
        self.target_name.clear();
        self.log("Detached: now editing a scratch mesh");
    }

    fn upload(&self, gs: &mut GameState, id: u32) {
        let src = self.eval.as_ref().unwrap_or(&self.mesh);
        let open = src.edges.iter().any(|e| e.f1 == NONE);
        let faces: Vec<Vec<usize>> = src
            .faces
            .iter()
            .map(|f| f.iter().map(|&v| v as usize).collect())
            .collect();
        gs.meshes.set_custom(id, src.verts.clone(), faces, open);
    }

    fn build_eval(&self) -> Option<EditMesh> {
        if !self.mirror.iter().any(|&b| b) && self.subsurf == 0 {
            return None;
        }
        let mut m = self.mesh.clone();
        m.select_none();
        for ax in 0..3 {
            if self.mirror[ax] {
                m.mirror(ax);
            }
        }
        for _ in 0..self.subsurf {
            if !m.subdivide() {
                break;
            }
        }
        Some(m)
    }

    /// false یعنی نتیجه هنوز آماده نیست (مش سنگین در حال Grab)
    fn rebuild_eval(&mut self) -> bool {
        if !self.mirror.iter().any(|&b| b) && self.subsurf == 0 {
            self.eval = None;
            return true;
        }
        let est = self.mesh.faces.len() * 4usize.pow(self.subsurf);
        if self.modal.is_some() && est > 20_000 {
            return false;
        }
        self.eval = self.build_eval();
        true
    }

    /// هر فریم بعد از UI صدا زده می‌شود
    pub fn sync(&mut self, gs: &mut GameState) {
        if !self.needs_sync {
            return;
        }
        self.needs_sync = false;
        if !self.rebuild_eval() {
            self.needs_sync = true;
            return;
        }
        if !self.live {
            return;
        }
        let Some(eid) = self.target else { return };
        if gs.scene.get(eid).is_none() {
            self.target = None;
            self.log("Target object was deleted: detached");
            return;
        }
        let id = match self.asset {
            Some(i) => i,
            None => {
                let i = gs.alloc_model_id();
                self.asset = Some(i);
                i
            }
        };
        self.upload(gs, id);
        gs.set_entity_model(eid, id, self.color);
    }

    pub fn apply_modifiers(&mut self) {
        if self.modal.is_some() {
            return;
        }
        if let Some(e) = self.build_eval() {
            self.push_undo();
            self.mesh = e;
            self.mirror = [false; 3];
            self.subsurf = 0;
            self.eval = None;
            self.log("Applied modifiers");
        } else {
            self.status = "No modifiers to apply".into();
        }
    }

    // =================================================================
    //  View helpers
    // =================================================================

    pub fn frame_all(&mut self) {
        if let Some((lo, hi)) = self.mesh.bounds() {
            self.cam.target = (lo + hi) * 0.5;
            self.cam.dist = ((hi - lo).length() * 1.5).clamp(2.0, 100.0);
        }
    }

    pub fn frame_selected(&mut self) {
        if self.mesh.sel_v.is_empty() {
            self.frame_all();
            return;
        }
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for &v in &self.mesh.sel_v {
            let p = self.mesh.verts[v as usize];
            lo = lo.min(p);
            hi = hi.max(p);
        }
        self.cam.target = (lo + hi) * 0.5;
        self.cam.dist = ((hi - lo).length() * 2.0).clamp(1.5, 100.0);
    }

    pub fn cursor_to_selected(&mut self) {
        self.cursor = self.mesh.median();
    }

    // =================================================================
    //  Undo
    // =================================================================

    pub fn push_undo(&mut self) {
        if self.undo.len() >= 40 {
            self.undo.remove(0);
        }
        self.undo.push(self.mesh.clone());
        self.redo.clear();
        self.needs_sync = true;
    }

    fn drop_undo(&mut self) {
        self.undo.pop();
    }

    pub fn undo(&mut self) {
        self.modal = None;
        if let Some(p) = self.undo.pop() {
            let cur = std::mem::replace(&mut self.mesh, p);
            self.redo.push(cur);
            self.needs_sync = true;
        }
    }

    pub fn redo(&mut self) {
        self.modal = None;
        if let Some(n) = self.redo.pop() {
            let cur = std::mem::replace(&mut self.mesh, n);
            self.undo.push(cur);
            self.needs_sync = true;
        }
    }

    // =================================================================
    //  Per-frame cache
    // =================================================================

    pub fn cache_ok(&self) -> bool {
        self.proj.len() == self.mesh.verts.len() && self.front.len() == self.mesh.faces.len()
    }

    pub fn prepare(&mut self, cam: &Cam) {
        let nv = self.mesh.verts.len();
        let nf = self.mesh.faces.len();
        self.open = self.mesh.edges.iter().any(|e| e.f1 == NONE);
        let show_all = self.xray || !self.cull || self.open;

        self.proj.clear();
        self.proj.extend(self.mesh.verts.iter().map(|&p| cam.project(p)));
        self.front.clear();
        self.fnorm.clear();
        self.fcen.clear();
        self.vis_v.clear();
        self.vis_v.resize(nv, false);
        self.vnorm.clear();
        if self.smooth_shade {
            self.vnorm.resize(nv, Vec3::ZERO);
        }
        for fi in 0..nf {
            let n = self.mesh.face_normal(fi);
            let c = self.mesh.face_center(fi);
            let fr = show_all || n.dot(cam.to_eye(c)) > 0.0;
            self.fnorm.push(n);
            self.fcen.push(c);
            self.front.push(fr);
            if fr {
                for &v in &self.mesh.faces[fi] {
                    self.vis_v[v as usize] = true;
                }
            }
        }
    }

    // =================================================================
    //  Selection
    // =================================================================

    pub fn set_mode(&mut self, m: SelMode) {
        if m == SelMode::Face {
            self.mesh.flush_from_faces();
        } else {
            self.mesh.flush_from_verts();
        }
        self.mode = m;
    }

    pub fn pick(&mut self, cam: &Cam, mouse: Vec2, extend: bool) {
        if !self.cache_ok() {
            return;
        }
        let m = &mut self.mesh;
        match self.mode {
            SelMode::Vertex => {
                let mut best: (f32, Option<u32>) = (14.0, None);
                for (i, p) in self.proj.iter().enumerate() {
                    if let Some(p) = p {
                        if !(self.xray || self.vis_v[i]) {
                            continue;
                        }
                        let d = (*p - mouse).length();
                        if d < best.0 {
                            best = (d, Some(i as u32));
                        }
                    }
                }
                match best.1 {
                    Some(v) => {
                        if extend {
                            if !m.sel_v.remove(&v) {
                                m.sel_v.insert(v);
                            }
                        } else {
                            m.sel_v.clear();
                            m.sel_v.insert(v);
                        }
                    }
                    None => {
                        if !extend {
                            m.sel_v.clear();
                        }
                    }
                }
                m.flush_from_verts();
            }
            SelMode::Edge => {
                let mut best: (f32, Option<usize>) = (10.0, None);
                for (ei, e) in m.edges.iter().enumerate() {
                    let vis = self.xray
                        || self.front[e.f0 as usize]
                        || (e.f1 != NONE && self.front[e.f1 as usize]);
                    if !vis {
                        continue;
                    }
                    if let (Some(a), Some(b)) = (self.proj[e.a as usize], self.proj[e.b as usize]) {
                        let d = dist_seg(mouse, a, b);
                        if d < best.0 {
                            best = (d, Some(ei));
                        }
                    }
                }
                match best.1 {
                    Some(ei) => {
                        let (a, b) = (m.edges[ei].a, m.edges[ei].b);
                        let both = m.sel_v.contains(&a) && m.sel_v.contains(&b);
                        if extend {
                            if both {
                                m.sel_v.remove(&a);
                                m.sel_v.remove(&b);
                            } else {
                                m.sel_v.insert(a);
                                m.sel_v.insert(b);
                            }
                        } else {
                            m.sel_v.clear();
                            m.sel_v.insert(a);
                            m.sel_v.insert(b);
                        }
                    }
                    None => {
                        if !extend {
                            m.sel_v.clear();
                        }
                    }
                }
                m.flush_from_verts();
            }
            SelMode::Face => {
                let mut best: (f32, Option<u32>) = (f32::MAX, None);
                let mut pts: Vec<Vec2> = Vec::new();
                for fi in 0..m.faces.len() {
                    if !self.front[fi] {
                        continue;
                    }
                    pts.clear();
                    for &v in &m.faces[fi] {
                        if let Some(p) = self.proj[v as usize] {
                            pts.push(p);
                        }
                    }
                    if pts.len() != m.faces[fi].len() || !point_in_poly(mouse, &pts) {
                        continue;
                    }
                    let d = cam.depth(self.fcen[fi]);
                    if d < best.0 {
                        best = (d, Some(fi as u32));
                    }
                }
                match best.1 {
                    Some(f) => {
                        if extend {
                            if !m.sel_f.remove(&f) {
                                m.sel_f.insert(f);
                            }
                        } else {
                            m.sel_f.clear();
                            m.sel_f.insert(f);
                        }
                    }
                    None => {
                        if !extend {
                            m.sel_f.clear();
                        }
                    }
                }
                m.flush_from_faces();
            }
        }
    }

    /// Alt+Click: انتخاب Edge Loop
    pub fn pick_loop(&mut self, mouse: Vec2, extend: bool) {
        if !self.cache_ok() {
            return;
        }
        let mut best: (f32, Option<usize>) = (14.0, None);
        for (ei, e) in self.mesh.edges.iter().enumerate() {
            let vis = self.xray
                || self.front[e.f0 as usize]
                || (e.f1 != NONE && self.front[e.f1 as usize]);
            if !vis {
                continue;
            }
            if let (Some(a), Some(b)) = (self.proj[e.a as usize], self.proj[e.b as usize]) {
                let d = dist_seg(mouse, a, b);
                if d < best.0 {
                    best = (d, Some(ei));
                }
            }
        }
        let Some(ei) = best.1 else { return };
        let verts = self.mesh.edge_loop(ei);
        self.mode = SelMode::Edge;
        if !extend {
            self.mesh.sel_v.clear();
        }
        self.mesh.sel_v.extend(verts);
        self.mesh.flush_from_verts();
    }

    pub fn box_select(&mut self, lo: Vec2, hi: Vec2, extend: bool) {
        if !self.cache_ok() {
            return;
        }
        let inside = |p: Vec2| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y;
        let m = &mut self.mesh;
        if !extend {
            m.sel_v.clear();
            m.sel_f.clear();
        }
        match self.mode {
            SelMode::Vertex => {
                for (i, p) in self.proj.iter().enumerate() {
                    if let Some(p) = p {
                        if (self.xray || self.vis_v[i]) && inside(*p) {
                            m.sel_v.insert(i as u32);
                        }
                    }
                }
                m.flush_from_verts();
            }
            SelMode::Edge => {
                for e in &m.edges {
                    let vis = self.xray
                        || self.front[e.f0 as usize]
                        || (e.f1 != NONE && self.front[e.f1 as usize]);
                    if !vis {
                        continue;
                    }
                    if let (Some(a), Some(b)) = (self.proj[e.a as usize], self.proj[e.b as usize]) {
                        if inside(a) && inside(b) {
                            m.sel_v.insert(e.a);
                            m.sel_v.insert(e.b);
                        }
                    }
                }
                m.flush_from_verts();
            }
            SelMode::Face => {
                for fi in 0..m.faces.len() {
                    if !self.front[fi] {
                        continue;
                    }
                    let mut s = Vec2::ZERO;
                    let mut ok = true;
                    for &v in &m.faces[fi] {
                        match self.proj[v as usize] {
                            Some(q) => s += q,
                            None => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok && inside(s / m.faces[fi].len() as f32) {
                        m.sel_f.insert(fi as u32);
                    }
                }
                m.flush_from_faces();
            }
        }
    }

    pub fn select_all(&mut self) {
        self.mesh.select_all();
    }
    pub fn select_none(&mut self) {
        self.mesh.select_none();
    }
    pub fn select_linked(&mut self) {
        if !self.mesh.select_linked() {
            self.status = "Select something first".into();
        }
    }
    pub fn select_more(&mut self) {
        self.mesh.select_more();
    }
    pub fn select_less(&mut self) {
        self.mesh.select_less();
    }
    pub fn select_invert(&mut self) {
        if self.mode == SelMode::Face {
            self.mesh.invert_faces();
        } else {
            self.mesh.invert_verts();
        }
    }

    // =================================================================
    //  Modal
    // =================================================================

    fn start_modal(
        &mut self,
        kind: ModalKind,
        constraint: Constraint,
        mouse: Vec2,
        inset: Vec<(u32, Vec3, Vec3)>,
    ) -> bool {
        let ids: Vec<u32> = self.mesh.sel_v.iter().copied().collect();
        if ids.is_empty() {
            return false;
        }
        let selp: Vec<Vec3> = ids.iter().map(|&i| self.mesh.verts[i as usize]).collect();
        let pivot = selp.iter().copied().sum::<Vec3>() / ids.len() as f32;
        let mut orig: Vec<(u32, Vec3, f32)> =
            ids.iter().zip(selp.iter()).map(|(&i, &p)| (i, p, 0.0)).collect();

        if self.proportional && kind != ModalKind::Inset {
            let n = self.mesh.verts.len();
            let brute = n * ids.len() <= 3_000_000;
            let r = self.prop_radius;
            for i in 0..n as u32 {
                if self.mesh.sel_v.contains(&i) {
                    continue;
                }
                let p = self.mesh.verts[i as usize];
                let d = if brute {
                    selp.iter()
                        .map(|q| (*q - p).length_squared())
                        .fold(f32::MAX, f32::min)
                        .sqrt()
                } else {
                    (pivot - p).length()
                };
                if d < r {
                    orig.push((i, p, d));
                }
            }
        }

        self.modal = Some(Modal {
            kind,
            constraint,
            start: mouse,
            pivot,
            normal: self.mesh.selection_normal(),
            orig,
            inset,
            num: String::new(),
        });
        true
    }

    pub fn begin_transform(&mut self, kind: ModalKind, mouse: Vec2) -> bool {
        if self.modal.is_some() {
            return false;
        }
        if self.mesh.sel_v.is_empty() {
            self.status = "Nothing selected".into();
            return false;
        }
        self.push_undo();
        if !self.start_modal(kind, Constraint::Free, mouse, Vec::new()) {
            self.drop_undo();
            return false;
        }
        true
    }

    pub fn op_extrude(&mut self, mouse: Vec2) {
        if self.modal.is_some() {
            return;
        }
        if self.mode == SelMode::Face && self.mesh.sel_f.is_empty() {
            self.status = "Extrude needs selected faces".into();
            return;
        }
        if self.mesh.sel_v.is_empty() {
            self.status = "Nothing selected".into();
            return;
        }
        self.push_undo();
        let (ok, cons) = if self.mode == SelMode::Face {
            (self.mesh.extrude_faces(), Constraint::Normal)
        } else {
            (self.mesh.extrude_edges(), Constraint::Free)
        };
        if ok && self.start_modal(ModalKind::Grab, cons, mouse, Vec::new()) {
            return;
        }
        self.drop_undo();
        self.status = "Nothing to extrude (select faces, or two connected vertices)".into();
    }

    pub fn op_inset(&mut self, mouse: Vec2) {
        if self.modal.is_some() {
            return;
        }
        if self.mesh.sel_f.is_empty() {
            self.status = "Inset needs selected faces".into();
            return;
        }
        self.push_undo();
        let list = self.mesh.inset_faces();
        if list.is_empty() || !self.start_modal(ModalKind::Inset, Constraint::Free, mouse, list) {
            self.cancel_modal();
        }
    }

    pub fn op_duplicate(&mut self, mouse: Vec2) {
        if self.modal.is_some() {
            return;
        }
        self.push_undo();
        if self.mesh.duplicate_faces() && self.start_modal(ModalKind::Grab, Constraint::Free, mouse, Vec::new()) {
            return;
        }
        self.drop_undo();
        self.status = "Duplicate needs selected faces".into();
    }

    pub fn toggle_constraint(&mut self, c: Constraint) {
        if let Some(m) = self.modal.as_mut() {
            m.constraint = if m.constraint == c { Constraint::Free } else { c };
            m.num.clear();
        }
    }

    pub fn modal_text(&mut self, s: &str) {
        if let Some(m) = self.modal.as_mut() {
            for c in s.chars() {
                if c.is_ascii_digit() || c == '.' || c == '-' {
                    m.num.push(c);
                }
            }
        }
    }

    pub fn modal_backspace(&mut self) {
        if let Some(m) = self.modal.as_mut() {
            m.num.pop();
        }
    }

    /// چرخ ماوس در Modal: شعاع Proportional Editing
    pub fn modal_scroll(&mut self, s: f32) {
        let prop = self.modal.as_ref().map_or(false, |m| m.kind != ModalKind::Inset);
        if self.proportional && prop {
            self.prop_radius = (self.prop_radius * (1.0 + s * 0.002)).clamp(0.05, 50.0);
        }
    }

    pub fn confirm_modal(&mut self) {
        self.modal = None;
        self.status.clear();
        self.needs_sync = true;
    }

    pub fn cancel_modal(&mut self) {
        self.modal = None;
        if let Some(p) = self.undo.pop() {
            self.mesh = p;
        }
        self.status = "Cancelled".into();
        self.needs_sync = true;
    }

    pub fn update_modal(&mut self, cam: &Cam, mouse: Vec2, snap: bool) {
        let Some(m) = self.modal.take() else { return };
        let rad = self.prop_radius;
        let pe = if self.proportional && m.kind != ModalKind::Inset {
            format!("  [PE r={rad:.2} · wheel]")
        } else {
            String::new()
        };
        let axis = match m.constraint {
            Constraint::X => Some(Vec3::X),
            Constraint::Y => Some(Vec3::Y),
            Constraint::Z => Some(Vec3::Z),
            Constraint::Normal => Some(m.normal),
            Constraint::Free => None,
        };
        let typed: Option<f32> = m.num.parse().ok();
        let cons = match m.constraint {
            Constraint::X => " X",
            Constraint::Y => " Y",
            Constraint::Z => " Z",
            Constraint::Normal => " Normal",
            Constraint::Free => "",
        };
        let ps = cam.project(m.pivot).unwrap_or(cam.min + cam.size * 0.5);

        match m.kind {
            ModalKind::Grab => {
                let delta = mouse - m.start;
                let off = if let Some(a) = axis {
                    let mut amount = typed.unwrap_or_else(|| {
                        match (cam.project(m.pivot), cam.project(m.pivot + a)) {
                            (Some(p0), Some(p1)) => {
                                let sv = p1 - p0;
                                let l2 = sv.length_squared();
                                if l2 < 1e-4 { 0.0 } else { delta.dot(sv) / l2 }
                            }
                            _ => 0.0,
                        }
                    });
                    if snap && typed.is_none() {
                        amount = (amount / 0.25).round() * 0.25;
                    }
                    self.status = format!("Grab{cons}: {amount:.3}{pe}   (type a number · Enter · Esc)");
                    a * amount
                } else {
                    let upp = cam.units_per_px(m.pivot);
                    let mut o = cam.right() * delta.x * upp - cam.up() * delta.y * upp;
                    if snap {
                        o = (o / 0.25).round() * 0.25;
                    }
                    self.status = format!("Grab: {:.3}, {:.3}, {:.3}{pe}   (X/Y/Z/N constrain)", o.x, o.y, o.z);
                    o
                };
                for (i, p, d) in &m.orig {
                    self.mesh.verts[*i as usize] = *p + off * weight(*d, rad);
                }
            }
            ModalKind::Rotate => {
                let s = m.start - ps;
                let c = mouse - ps;
                let mut ang = c.y.atan2(c.x) - s.y.atan2(s.x);
                while ang > std::f32::consts::PI {
                    ang -= std::f32::consts::TAU;
                }
                while ang < -std::f32::consts::PI {
                    ang += std::f32::consts::TAU;
                }
                let axv = axis
                    .unwrap_or_else(|| cam.to_eye(m.pivot).normalize_or_zero())
                    .normalize_or_zero();
                let toward = axv.dot(cam.to_eye(m.pivot)) >= 0.0;
                let mut w = if toward { -ang } else { ang };
                if let Some(t) = typed {
                    w = t.to_radians();
                } else if snap {
                    w = ((w.to_degrees() / 15.0).round() * 15.0).to_radians();
                }
                self.status = format!("Rotate{cons}: {:.1}°{pe}", w.to_degrees());
                if axv != Vec3::ZERO {
                    let q = Quat::from_axis_angle(axv, w);
                    for (i, p, d) in &m.orig {
                        let np = m.pivot + q * (*p - m.pivot);
                        self.mesh.verts[*i as usize] = p.lerp(np, weight(*d, rad));
                    }
                }
            }
            ModalKind::Scale => {
                let d0 = (m.start - ps).length().max(10.0);
                let d1 = (mouse - ps).length();
                let mut f = typed.unwrap_or(d1 / d0);
                if snap && typed.is_none() {
                    f = (f / 0.1).round() * 0.1;
                }
                self.status = format!("Scale{cons}: {f:.3}{pe}");
                for (i, p, d) in &m.orig {
                    let dv = *p - m.pivot;
                    let np = match axis {
                        Some(a) => m.pivot + dv + a * (a.dot(dv) * (f - 1.0)),
                        None => m.pivot + dv * f,
                    };
                    self.mesh.verts[*i as usize] = p.lerp(np, weight(*d, rad));
                }
            }
            ModalKind::Inset => {
                let f = typed.unwrap_or((mouse - m.start).length() / 300.0).clamp(0.0, 0.95);
                self.status = format!("Inset: {f:.2}");
                for (i, outer, cen) in &m.inset {
                    self.mesh.verts[*i as usize] = *outer + (*cen - *outer) * f;
                }
            }
        }
        self.needs_sync = true;
        self.modal = Some(m);
    }

    // =================================================================
    //  Operators
    // =================================================================

    fn apply(&mut self, name: &str, f: impl FnOnce(&mut EditMesh) -> bool) {
        if self.modal.is_some() {
            return;
        }
        self.push_undo();
        if f(&mut self.mesh) {
            self.log(name.to_string());
        } else {
            self.drop_undo();
            self.status = format!("{name}: nothing to do");
        }
    }

    pub fn op_loop_cut(&mut self) {
        let n = self.cuts;
        self.apply("Loop Cut", move |m| m.loop_cut(n));
    }
    pub fn op_subdivide(&mut self) {
        self.apply("Subdivide", |m| m.subdivide());
    }
    pub fn op_delete(&mut self) {
        let mode = self.mode;
        self.apply("Delete", move |m| match mode {
            SelMode::Vertex => m.delete_verts(),
            SelMode::Edge => m.delete_edges(),
            SelMode::Face => m.delete_faces(),
        });
    }
    pub fn op_merge_center(&mut self) {
        self.apply("Merge at Center", |m| m.merge_center());
    }
    pub fn op_weld(&mut self) {
        let eps = self.weld_eps;
        self.apply("Merge by Distance", move |m| m.weld(eps) > 0);
    }
    pub fn op_flip(&mut self) {
        self.apply("Flip Normals", |m| m.flip_normals());
    }
    pub fn op_recalc(&mut self) {
        self.apply("Recalculate Normals", |m| {
            m.recalc_outside();
            true
        });
    }
    pub fn op_triangulate(&mut self) {
        self.apply("Triangulate", |m| m.triangulate());
    }
    pub fn op_smooth(&mut self) {
        let (i, a) = (self.smooth_iter, self.smooth_amt);
        self.apply("Smooth", move |m| m.smooth(i, a));
    }
    pub fn op_mirror(&mut self, axis: usize) {
        self.apply("Mirror (destructive)", move |m| m.mirror(axis));
    }
    pub fn op_fill(&mut self) {
        self.apply("Fill", |m| m.fill());
    }
    pub fn op_dissolve(&mut self) {
        self.apply("Dissolve Edges", |m| m.dissolve_edges());
    }
    pub fn op_poke(&mut self) {
        self.apply("Poke Faces", |m| m.poke());
    }
    pub fn op_solidify(&mut self) {
        let t = self.solidify_t;
        self.apply("Solidify", move |m| m.solidify(t));
    }
    pub fn op_to_sphere(&mut self) {
        let f = self.sphere_f;
        self.apply("To Sphere", move |m| m.to_sphere(f));
    }
    pub fn op_recenter(&mut self) {
        self.apply("Origin to Center", |m| m.recenter());
    }
    pub fn op_origin_bottom(&mut self) {
        self.apply("Origin to Bottom", |m| m.origin_to_bottom());
    }

    /// وجه‌های انتخاب‌شده به یک آبجکت جدید در صحنه تبدیل می‌شوند
    pub fn op_separate(&mut self, gs: &mut GameState) {
        if self.modal.is_some() || gs.mode != PlayMode::Edit {
            return;
        }
        self.push_undo();
        match self.mesh.separate_faces() {
            Some(part) => {
                let id = gs.alloc_model_id();
                let open = part.edges.iter().any(|e| e.f1 == NONE);
                let faces: Vec<Vec<usize>> = part
                    .faces
                    .iter()
                    .map(|f| f.iter().map(|&v| v as usize).collect())
                    .collect();
                gs.meshes.set_custom(id, part.verts, faces, open);
                let name = if self.target_name.is_empty() {
                    "Part".to_string()
                } else {
                    format!("{} (part)", self.target_name)
                };
                let eid = gs.create(&name, vec![Component::Mesh { prim: Primitive::Custom(id), color: self.color }], None);
                let tr = self
                    .target
                    .and_then(|t| gs.scene.get(t))
                    .map(|e| (e.pos, e.rot, e.scale, e.parent));
                if let Some((pos, rot, scale, parent)) = tr {
                    if let Some(n) = gs.scene.get_mut(eid) {
                        n.pos = pos;
                        n.rot = rot;
                        n.scale = scale;
                        n.parent = parent;
                    }
                }
                self.log(format!("Separated into '{name}'"));
            }
            None => {
                self.drop_undo();
                self.status = "Separate needs a partial face selection".into();
            }
        }
    }

    pub fn translate_selection(&mut self, off: Vec3) {
        let ids: Vec<u32> = self.mesh.sel_v.iter().copied().collect();
        for i in ids {
            self.mesh.verts[i as usize] += off;
        }
        self.needs_sync = true;
    }

    pub fn add_primitive(&mut self, k: PrimKind) {
        if self.modal.is_some() {
            return;
        }
        if self.mesh.verts.len() > 300_000 {
            self.log("Mesh too large to add more");
            return;
        }
        self.push_undo();
        let nv0 = self.mesh.verts.len();
        let mut p = match k {
            PrimKind::Cube => mesh::cube(),
            PrimKind::Grid => mesh::grid(8),
            PrimKind::Sphere => mesh::uv_sphere(24, 12),
            PrimKind::Cylinder => mesh::cylinder(24),
            PrimKind::Cone => mesh::cone(24),
            PrimKind::Torus => mesh::torus(32, 16),
        };
        let s = self.add_size * 0.5;
        for v in p.verts.iter_mut() {
            *v = *v * s + self.cursor;
        }
        self.mesh.append(&p);
        self.mesh.sel_v.clear();
        for i in nv0..self.mesh.verts.len() {
            self.mesh.sel_v.insert(i as u32);
        }
        self.mesh.flush_from_verts();
        self.log("Added primitive at 3D cursor");
        if self.grab_on_add {
            self.cmd = Some(Cmd::Grab);
        }
    }

    pub fn clear_mesh(&mut self) {
        if self.modal.is_some() {
            return;
        }
        self.push_undo();
        self.mesh = EditMesh::default();
        self.log("Cleared mesh");
    }

    // =================================================================
    //  OBJ
    // =================================================================

    pub fn export_obj(&mut self) {
        let path = self.obj_path.clone();
        if let Some(parent) = Path::new(&path).parent() {
            let _ = fs::create_dir_all(parent);
        }
        let src = self.eval.as_ref().unwrap_or(&self.mesh);
        match fs::write(&path, src.to_obj()) {
            Ok(()) => self.log(format!("Exported {path}")),
            Err(e) => self.log(format!("Export failed: {e}")),
        }
    }

    pub fn import_obj(&mut self) {
        let path = self.obj_path.clone();
        match fs::read_to_string(&path) {
            Ok(s) => {
                let m = EditMesh::from_obj(&s);
                if m.faces.is_empty() {
                    self.log("OBJ has no faces");
                    return;
                }
                self.push_undo();
                self.mesh = m;
                self.frame_all();
                self.log(format!("Imported {path}"));
            }
            Err(e) => self.log(format!("Import failed: {e}")),
        }
    }
}