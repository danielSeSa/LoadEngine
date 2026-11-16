pub mod mesh;

use std::collections::{HashMap, HashSet};

use glam::{EulerRot, Mat4, Quat, Vec3};
use mesh::{MeshLib, Primitive};

pub type EntityId = u64;

// =====================================================================
//  Components & Entities
// =====================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LightKind {
    Directional,
    Point,
}

impl LightKind {
    pub fn name(self) -> &'static str {
        match self {
            LightKind::Directional => "Directional",
            LightKind::Point => "Point",
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum Component {
    Mesh { prim: Primitive, color: [f32; 3] },
    Light { kind: LightKind, color: [f32; 3], intensity: f32 },
    Camera { fov: f32 },
    Script { path: String },
    Spin { speed: f32 },
    Rigidbody { gravity: bool, bounce: f32 },
}

impl Component {
    pub fn name(&self) -> &'static str {
        match self {
            Component::Mesh { .. } => "Mesh Renderer",
            Component::Light { .. } => "Light",
            Component::Camera { .. } => "Camera",
            Component::Script { .. } => "Script",
            Component::Spin { .. } => "Spin",
            Component::Rigidbody { .. } => "Rigidbody",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Component::Mesh { .. } => "📦",
            Component::Light { .. } => "💡",
            Component::Camera { .. } => "📷",
            Component::Script { .. } => "📜",
            Component::Spin { .. } => "⟳",
            Component::Rigidbody { .. } => "⚙",
        }
    }

    pub fn templates() -> Vec<Component> {
        vec![
            Component::Mesh { prim: Primitive::Cube, color: [0.8, 0.8, 0.8] },
            Component::Light { kind: LightKind::Point, color: [1.0, 1.0, 1.0], intensity: 1.0 },
            Component::Camera { fov: 60.0 },
            Component::Script { path: String::new() },
            Component::Spin { speed: 45.0 },
            Component::Rigidbody { gravity: true, bounce: 0.4 },
        ]
    }
}

#[derive(Clone, PartialEq)]
pub struct Entity {
    pub id: EntityId,
    pub name: String,
    pub parent: Option<EntityId>,
    pub pos: Vec3,
    /// درجه (Euler، ترتیب YXZ)
    pub rot: Vec3,
    pub scale: Vec3,
    pub active: bool,
    pub components: Vec<Component>,
}

impl Entity {
    pub fn quat(&self) -> Quat {
        Quat::from_euler(
            EulerRot::YXZ,
            self.rot.y.to_radians(),
            self.rot.x.to_radians(),
            self.rot.z.to_radians(),
        )
    }

    pub fn local_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.quat(), self.pos)
    }

    pub fn mesh(&self) -> Option<(Primitive, [f32; 3])> {
        self.components.iter().find_map(|c| match c {
            Component::Mesh { prim, color } => Some((*prim, *color)),
            _ => None,
        })
    }

    pub fn icon(&self) -> &'static str {
        if let Some((p, _)) = self.mesh() {
            return p.icon();
        }
        for c in &self.components {
            match c {
                Component::Camera { .. } => return "📷",
                Component::Light { .. } => return "💡",
                _ => {}
            }
        }
        "◇"
    }
}

// =====================================================================
//  Scene
// =====================================================================

#[derive(Clone, PartialEq)]
pub struct Scene {
    pub entities: Vec<Entity>,
    next_id: EntityId,
}

impl Scene {
    pub fn demo() -> Self {
        let mut s = Scene { entities: Vec::new(), next_id: 1 };

        let cam = s.spawn("Main Camera", vec![Component::Camera { fov: 60.0 }], Vec3::new(0.0, 2.5, 7.0));
        s.get_mut(cam).unwrap().rot = Vec3::new(-15.0, 0.0, 0.0);

        let sun = s.spawn(
            "Directional Light",
            vec![Component::Light { kind: LightKind::Directional, color: [1.0, 0.96, 0.88], intensity: 1.0 }],
            Vec3::new(0.0, 6.0, 0.0),
        );
        s.get_mut(sun).unwrap().rot = Vec3::new(-50.0, -30.0, 0.0);

        let ground = s.spawn(
            "Ground",
            vec![Component::Mesh { prim: Primitive::Plane, color: Primitive::Plane.default_color() }],
            Vec3::ZERO,
        );
        s.get_mut(ground).unwrap().scale = Vec3::new(6.0, 1.0, 6.0);

        s.spawn(
            "Player Cube",
            vec![
                Component::Mesh { prim: Primitive::Cube, color: Primitive::Cube.default_color() },
                Component::Rigidbody { gravity: true, bounce: 0.5 },
                Component::Spin { speed: 40.0 },
            ],
            Vec3::new(-1.8, 3.0, 0.0),
        );

        s.spawn(
            "Bouncy Ball",
            vec![
                Component::Mesh { prim: Primitive::Sphere, color: Primitive::Sphere.default_color() },
                Component::Rigidbody { gravity: true, bounce: 0.75 },
            ],
            Vec3::new(1.8, 4.5, 0.5),
        );

        let pyr = s.spawn(
            "Pyramid",
            vec![Component::Mesh { prim: Primitive::Pyramid, color: Primitive::Pyramid.default_color() }],
            Vec3::new(0.0, 0.5, -2.5),
        );
        let gem = s.spawn(
            "Gem (child)",
            vec![
                Component::Mesh { prim: Primitive::Sphere, color: [0.6, 0.9, 1.0] },
                Component::Spin { speed: 90.0 },
            ],
            Vec3::new(0.0, 1.0, 0.0),
        );
        let g = s.get_mut(gem).unwrap();
        g.parent = Some(pyr);
        g.scale = Vec3::splat(0.4);
        s
    }

    fn alloc(&mut self) -> EntityId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn spawn(&mut self, name: &str, components: Vec<Component>, pos: Vec3) -> EntityId {
        let id = self.alloc();
        self.entities.push(Entity {
            id,
            name: name.to_string(),
            parent: None,
            pos,
            rot: Vec3::ZERO,
            scale: Vec3::ONE,
            active: true,
            components,
        });
        id
    }

    pub fn get(&self, id: EntityId) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }

    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|e| e.id == id)
    }

    pub fn children(&self, id: EntityId) -> Vec<EntityId> {
        self.entities.iter().filter(|e| e.parent == Some(id)).map(|e| e.id).collect()
    }

    pub fn roots(&self) -> Vec<EntityId> {
        self.entities
            .iter()
            .filter(|e| e.parent.map_or(true, |p| self.get(p).is_none()))
            .map(|e| e.id)
            .collect()
    }

    /// ترتیب DFS همراه با عمق
    pub fn dfs_order(&self) -> Vec<(EntityId, usize)> {
        let mut out = Vec::new();
        for r in self.roots() {
            self.dfs(r, 0, &mut out);
        }
        out
    }

    fn dfs(&self, id: EntityId, depth: usize, out: &mut Vec<(EntityId, usize)>) {
        out.push((id, depth));
        if depth > 32 {
            return;
        }
        for c in self.children(id) {
            self.dfs(c, depth + 1, out);
        }
    }

    pub fn world_matrix(&self, id: EntityId) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        let mut cur = Some(id);
        let mut guard = 0;
        while let Some(c) = cur {
            let Some(e) = self.get(c) else { break };
            m = e.local_matrix() * m;
            cur = e.parent;
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        m
    }

    /// آبجکت و همه‌ی والدهایش فعال باشند
    pub fn is_visible(&self, id: EntityId) -> bool {
        let mut cur = Some(id);
        let mut guard = 0;
        while let Some(c) = cur {
            let Some(e) = self.get(c) else { return true };
            if !e.active {
                return false;
            }
            cur = e.parent;
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        true
    }

    pub fn is_descendant(&self, id: EntityId, ancestor: EntityId) -> bool {
        let mut cur = self.get(id).and_then(|e| e.parent);
        let mut guard = 0;
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = self.get(c).and_then(|e| e.parent);
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        false
    }

    pub fn remove(&mut self, id: EntityId) {
        let mut kill = vec![id];
        let mut i = 0;
        while i < kill.len() {
            let cur = kill[i];
            kill.extend(self.children(cur));
            i += 1;
        }
        self.entities.retain(|e| !kill.contains(&e.id));
    }

    pub fn duplicate(&mut self, id: EntityId) -> Option<EntityId> {
        let parent = self.get(id)?.parent;
        self.dup_tree(id, parent, true)
    }

    fn dup_tree(&mut self, src: EntityId, parent: Option<EntityId>, root: bool) -> Option<EntityId> {
        let mut e = self.get(src)?.clone();
        let id = self.alloc();
        e.id = id;
        e.parent = parent;
        if root {
            e.name = format!("{} (Copy)", e.name);
            e.pos.x += 0.7;
        }
        self.entities.push(e);
        for c in self.children(src) {
            self.dup_tree(c, Some(id), false);
        }
        Some(id)
    }

    /// تغییر والد با حفظ موقعیت جهانی
    pub fn set_parent(&mut self, id: EntityId, new_parent: Option<EntityId>) {
        if let Some(p) = new_parent {
            if p == id || self.is_descendant(p, id) {
                return;
            }
        }
        let world = self.world_matrix(id);
        let parent_inv = new_parent
            .map(|p| self.world_matrix(p).inverse())
            .unwrap_or(Mat4::IDENTITY);
        let local = parent_inv * world;
        let (s, r, t) = local.to_scale_rotation_translation();
        let (ey, ex, ez) = r.to_euler(EulerRot::YXZ);
        if let Some(e) = self.get_mut(id) {
            e.pos = t;
            e.rot = Vec3::new(ex.to_degrees(), ey.to_degrees(), ez.to_degrees());
            e.scale = s;
            e.parent = new_parent;
        }
    }
}

// =====================================================================
//  Camera / Tools / Modes
// =====================================================================

#[derive(Clone)]
pub struct OrbitCam {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub target: Vec3,
    pub fov: f32,
}

impl OrbitCam {
    pub fn new() -> Self {
        Self { yaw: 0.6, pitch: 0.45, dist: 9.0, target: Vec3::new(0.0, 0.8, 0.0), fov: 60.0 }
    }

    pub fn eye(&self) -> Vec3 {
        self.target
            + Vec3::new(
                self.pitch.cos() * self.yaw.sin(),
                self.pitch.sin(),
                self.pitch.cos() * self.yaw.cos(),
            ) * self.dist
    }

    pub fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.target, Vec3::Y)
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.008;
        self.pitch = (self.pitch + dy * 0.008).clamp(-1.5, 1.5);
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        let f = (self.target - self.eye()).normalize();
        let r = f.cross(Vec3::Y).normalize();
        let u = r.cross(f);
        let s = self.dist * 0.0016;
        self.target += -r * dx * s + u * dy * s;
    }

    pub fn zoom(&mut self, scroll: f32) {
        self.dist = (self.dist * (1.0 - scroll * 0.0015)).clamp(0.5, 300.0);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Move,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Shading {
    Solid,
    Wire,
    SolidWire,
}

impl Shading {
    pub const ALL: [Shading; 3] = [Shading::Solid, Shading::Wire, Shading::SolidWire];
    pub fn label(self) -> &'static str {
        match self {
            Shading::Solid => "Solid",
            Shading::Wire => "Wireframe",
            Shading::SolidWire => "Solid + Wire",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    Edit,
    Playing,
    Paused,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BottomTab {
    Project,
    Console,
}

#[derive(Clone)]
pub struct GizmoDrag {
    pub axis: usize,
    pub start_pos: Vec3,
    pub start_rot: Vec3,
    pub start_scale: Vec3,
    pub acc: f32,
}

// =====================================================================
//  GameState
// =====================================================================

pub struct GameState {
    pub scene: Scene,
    pub selected: Option<EntityId>,
    pub tool: Tool,
    pub shading: Shading,
    pub show_grid: bool,
    pub snap: bool,
    pub cam: OrbitCam,
    pub meshes: MeshLib,
    pub undo: Vec<Scene>,
    pub redo: Vec<Scene>,
    pub mode: PlayMode,
    pub time: f32,
    pub bottom_tab: BottomTab,
    pub filter: String,
    pub log: Vec<String>,
    pub gizmo: Option<GizmoDrag>,
    pub fps: f32,
    pub stat_faces: usize,
    pub collapsed: HashSet<EntityId>,
    backup: Option<Scene>,
    before: Option<Scene>,
    in_gesture: bool,
    vel: HashMap<EntityId, f32>,
    pub edit_request: Option<EntityId>,
    next_model: u32,
}

impl GameState {
    pub fn new() -> Self {
        let scene = Scene::demo();
        let selected = scene.entities.iter().find(|e| e.name == "Player Cube").map(|e| e.id);
        let mut gs = Self {
            scene,
            selected,
            tool: Tool::Move,
            shading: Shading::Solid,
            show_grid: true,
            snap: false,
            cam: OrbitCam::new(),
            meshes: MeshLib::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            mode: PlayMode::Edit,
            time: 0.0,
            bottom_tab: BottomTab::Project,
            filter: String::new(),
            log: Vec::new(),
            gizmo: None,
            fps: 60.0,
            stat_faces: 0,
            collapsed: HashSet::new(),
            backup: None,
            before: None,
            in_gesture: false,
            vel: HashMap::new(),
            edit_request: None,
            next_model: 1,
        };
        gs.log("LoadEngine ready. Press Ctrl+P to play the scene.");
        gs
    }

        pub fn alloc_model_id(&mut self) -> u32 {
        let id = self.next_model;
        self.next_model += 1;
        id
    }

    pub fn model_users(&self, id: u32) -> usize {
        self.scene
            .entities
            .iter()
            .filter(|e| matches!(e.mesh(), Some((Primitive::Custom(i), _)) if i == id))
            .count()
    }

    /// مش آبجکت را به مدل سفارشی وصل می‌کند (بار اول در Undo بازی ثبت می‌شود)
    pub fn set_entity_model(&mut self, eid: EntityId, model: u32, color: [f32; 3]) {
        let target = Primitive::Custom(model);
        let first = match self.scene.get(eid) {
            Some(e) => e.mesh().map_or(true, |(p, _)| p != target),
            None => return,
        };
        if first && self.mode == PlayMode::Edit {
            self.undo.push(self.scene.clone());
            self.redo.clear();
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
        }
        if let Some(e) = self.scene.get_mut(eid) {
            let mut found = false;
            for c in e.components.iter_mut() {
                if let Component::Mesh { prim, color: col } = c {
                    *prim = target;
                    *col = color;
                    found = true;
                    break;
                }
            }
            if !found {
                e.components.push(Component::Mesh { prim: target, color });
            }
        }
    }

    pub fn log(&mut self, msg: impl Into<String>) {
        if self.log.len() > 500 {
            self.log.drain(..100);
        }
        self.log.push(msg.into());
    }

    // ---------- Undo / Redo ----------

    pub fn begin_frame(&mut self) {
        self.before = if self.mode == PlayMode::Edit { Some(self.scene.clone()) } else { None };
    }

    pub fn commit(&mut self, pointer_down: bool) {
        if self.mode != PlayMode::Edit {
            self.before = None;
            return;
        }
        if let Some(before) = self.before.take() {
            if before != self.scene {
                if !self.in_gesture {
                    self.undo.push(before);
                    if self.undo.len() > 100 {
                        self.undo.remove(0);
                    }
                    self.redo.clear();
                }
                self.in_gesture = pointer_down;
            } else if !pointer_down {
                self.in_gesture = false;
            }
        }
    }

    fn after_history(&mut self) {
        self.before = None;
        if let Some(s) = self.selected {
            if self.scene.get(s).is_none() {
                self.selected = None;
            }
        }
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            let cur = std::mem::replace(&mut self.scene, prev);
            self.redo.push(cur);
            self.after_history();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            let cur = std::mem::replace(&mut self.scene, next);
            self.undo.push(cur);
            self.after_history();
        }
    }

    // ---------- Editing ----------

    pub fn create(&mut self, name: &str, comps: Vec<Component>, parent: Option<EntityId>) -> EntityId {
        let t = self.cam.target;
        let pos = if parent.is_some() { Vec3::ZERO } else { Vec3::new(t.x.round(), t.y, t.z.round()) };
        let id = self.scene.spawn(name, comps, pos);
        if let Some(e) = self.scene.get_mut(id) {
            e.parent = parent;
        }
        self.selected = Some(id);
        self.log(format!("Created '{name}'"));
        id
    }

    pub fn create_primitive(&mut self, p: Primitive, parent: Option<EntityId>) -> EntityId {
        let id = self.create(
            p.name(),
            vec![Component::Mesh { prim: p, color: p.default_color() }],
            parent,
        );
        if p == Primitive::Plane {
            if let Some(e) = self.scene.get_mut(id) {
                e.scale = Vec3::new(3.0, 1.0, 3.0);
            }
        }
        id
    }

    pub fn delete(&mut self, id: EntityId) {
        let name = self.scene.get(id).map(|e| e.name.clone()).unwrap_or_default();
        self.scene.remove(id);
        if let Some(s) = self.selected {
            if self.scene.get(s).is_none() {
                self.selected = None;
            }
        }
        self.log(format!("Deleted '{name}'"));
    }

    pub fn duplicate(&mut self, id: EntityId) {
        if let Some(n) = self.scene.duplicate(id) {
            self.selected = Some(n);
        }
    }

    pub fn focus(&mut self, id: EntityId) {
        if self.scene.get(id).is_some() {
            let w = self.scene.world_matrix(id);
            self.cam.target = w.transform_point3(Vec3::ZERO);
            let (s, _, _) = w.to_scale_rotation_translation();
            self.cam.dist = (s.max_element() * 3.0).clamp(3.0, 40.0);
        }
    }

    // ---------- Play mode ----------

    pub fn play(&mut self) {
        if self.mode == PlayMode::Edit {
            self.backup = Some(self.scene.clone());
            self.vel.clear();
            self.time = 0.0;
            self.gizmo = None;
            let mut lines = Vec::new();
            for e in &self.scene.entities {
                for c in &e.components {
                    if let Component::Script { path } = c {
                        if !path.is_empty() {
                            lines.push(format!(
                                "[Script] '{path}' attached to '{}' (script interpreter not wired yet)",
                                e.name
                            ));
                        }
                    }
                }
            }
            self.log("▶ Play started");
            for l in lines {
                self.log(l);
            }
        }
        self.mode = PlayMode::Playing;
        self.before = None;
    }

    pub fn pause(&mut self) {
        if self.mode == PlayMode::Playing {
            self.mode = PlayMode::Paused;
        }
    }

    pub fn resume(&mut self) {
        if self.mode == PlayMode::Paused {
            self.mode = PlayMode::Playing;
        }
    }

    pub fn stop(&mut self) {
        if let Some(b) = self.backup.take() {
            self.scene = b;
        }
        self.mode = PlayMode::Edit;
        self.before = None;
        self.vel.clear();
        if let Some(s) = self.selected {
            if self.scene.get(s).is_none() {
                self.selected = None;
            }
        }
        self.log("⏹ Play stopped, scene restored");
    }

    pub fn step(&mut self, dt: f32) {
        if self.mode != PlayMode::Playing {
            return;
        }
        self.time += dt;
        for e in self.scene.entities.iter_mut() {
            if !e.active {
                continue;
            }
            let prim = e.components.iter().find_map(|c| match c {
                Component::Mesh { prim, .. } => Some(*prim),
                _ => None,
            });
            let half = match prim {
                Some(Primitive::Plane) | None => 0.0,
                Some(p @ Primitive::Custom(_)) => {
                    let lo = self
                        .meshes
                        .get(p)
                        .verts
                        .iter()
                        .map(|v| v.y)
                        .fold(f32::MAX, f32::min);
                    if lo == f32::MAX { 0.0 } else { -lo * e.scale.y }
                }
                Some(_) => 0.5 * e.scale.y,
            };
            for c in e.components.iter() {
                match c {
                    Component::Spin { speed } => {
                        e.rot.y = (e.rot.y + speed * dt).rem_euclid(360.0);
                    }
                    Component::Rigidbody { gravity, bounce } if e.parent.is_none() => {
                        let v = self.vel.entry(e.id).or_insert(0.0);
                        if *gravity {
                            *v -= 9.81 * dt;
                        }
                        e.pos.y += *v * dt;
                        if e.pos.y < half {
                            e.pos.y = half;
                            if v.abs() > 0.8 {
                                *v = -*v * *bounce;
                            } else {
                                *v = 0.0;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}