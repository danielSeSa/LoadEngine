use std::f32::consts::TAU;

use egui::{
    pos2, vec2, Align2, Color32, CursorIcon, FontId, Key, Painter, PointerButton, Pos2, Rect,
    Sense, Shape, Stroke, Vec2,
};
use glam::{Mat4, Vec3};

use crate::game::{
    Component, EntityId, GameState, GizmoDrag, LightKind, PlayMode, Shading, Tool,
};

const AXIS_COLORS: [Color32; 3] = [
    Color32::from_rgb(235, 75, 75),
    Color32::from_rgb(110, 205, 85),
    Color32::from_rgb(75, 125, 245),
];
const HOT: Color32 = Color32::from_rgb(255, 220, 60);
const SELECT: Color32 = Color32::from_rgb(255, 150, 40);

// =====================================================================
//  View (projection helpers)
// =====================================================================

struct View {
    view: Mat4,
    proj: Mat4,
    vp: Mat4,
    eye: Vec3,
    rect: Rect,
}

const NEAR: f32 = -0.05;

impl View {
    fn to_screen(&self, nx: f32, ny: f32) -> Pos2 {
        pos2(
            self.rect.left() + (nx * 0.5 + 0.5) * self.rect.width(),
            self.rect.top() + (0.5 - ny * 0.5) * self.rect.height(),
        )
    }

    fn proj_view(&self, v: Vec3) -> Pos2 {
        let c = self.proj * v.extend(1.0);
        let n = c / c.w;
        self.to_screen(n.x, n.y)
    }

    fn point(&self, p: Vec3) -> Option<Pos2> {
        let v = self.view * p.extend(1.0);
        if v.z > NEAR {
            return None;
        }
        Some(self.proj_view(v.truncate()))
    }

    fn line(&self, a: Vec3, b: Vec3) -> Option<[Pos2; 2]> {
        let mut va = (self.view * a.extend(1.0)).truncate();
        let mut vb = (self.view * b.extend(1.0)).truncate();
        if va.z > NEAR && vb.z > NEAR {
            return None;
        }
        if va.z > NEAR {
            let t = (NEAR - va.z) / (vb.z - va.z);
            va = va + (vb - va) * t;
        } else if vb.z > NEAR {
            let t = (NEAR - vb.z) / (va.z - vb.z);
            vb = vb + (va - vb) * t;
        }
        Some([self.proj_view(va), self.proj_view(vb)])
    }

    fn ray(&self, p: Pos2) -> (Vec3, Vec3) {
        let nx = (p.x - self.rect.left()) / self.rect.width() * 2.0 - 1.0;
        let ny = 1.0 - (p.y - self.rect.top()) / self.rect.height() * 2.0;
        let inv = self.vp.inverse();
        let a = inv.project_point3(Vec3::new(nx, ny, 0.0));
        let b = inv.project_point3(Vec3::new(nx, ny, 1.0));
        (a, (b - a).normalize())
    }
}

fn dist_seg(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    let t = if l2 < 1e-6 { 0.0 } else { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) };
    (p - (a + ab * t)).length()
}

// =====================================================================
//  Picking
// =====================================================================

fn ray_tri(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let t = o - a;
    let u = t.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let tt = e2.dot(q) * inv;
    if tt > 1e-4 { Some(tt) } else { None }
}

fn pick(gs: &GameState, v: &View, pos: Pos2) -> Option<EntityId> {
    // آیکون نور و دوربین
    let mut best_icon: Option<(f32, EntityId)> = None;
    for e in &gs.scene.entities {
        if !gs.scene.is_visible(e.id) || e.mesh().is_some() {
            continue;
        }
        let has = e
            .components
            .iter()
            .any(|c| matches!(c, Component::Camera { .. } | Component::Light { .. }));
        if !has {
            continue;
        }
        let w = gs.scene.world_matrix(e.id).transform_point3(Vec3::ZERO);
        if let Some(s) = v.point(w) {
            let d = (s - pos).length();
            if d < 16.0 && best_icon.map_or(true, |b| d < b.0) {
                best_icon = Some((d, e.id));
            }
        }
    }
    if let Some((_, id)) = best_icon {
        return Some(id);
    }

    // مش‌ها
    let (o, d) = v.ray(pos);
    let mut best: Option<(f32, EntityId)> = None;
    for e in &gs.scene.entities {
        if !gs.scene.is_visible(e.id) {
            continue;
        }
        let Some((prim, _)) = e.mesh() else { continue };
        let world = gs.scene.world_matrix(e.id);
        let inv = world.inverse();
        let lo = inv.transform_point3(o);
        let ld = inv.transform_vector3(d);
        let mesh = gs.meshes.get(prim);
        for f in &mesh.faces {
            for k in 1..f.len() - 1 {
                if let Some(t) = ray_tri(lo, ld, mesh.verts[f[0]], mesh.verts[f[k]], mesh.verts[f[k + 1]]) {
                    if best.map_or(true, |b| t < b.0) {
                        best = Some((t, e.id));
                    }
                }
            }
        }
    }
    best.map(|b| b.1)
}

// =====================================================================
//  Gizmo
// =====================================================================

struct GizmoInfo {
    origin: Vec3,
    size: f32,
    axes: [Vec3; 3],
}

fn gizmo_info(gs: &GameState, eye: Vec3) -> Option<GizmoInfo> {
    if gs.tool == Tool::Select {
        return None;
    }
    let sel = gs.selected?;
    gs.scene.get(sel)?;
    let w = gs.scene.world_matrix(sel);
    let origin = w.transform_point3(Vec3::ZERO);
    let size = ((eye - origin).length() * 0.16).max(0.1);
    let axes = if gs.tool == Tool::Scale {
        [
            w.transform_vector3(Vec3::X).normalize_or_zero(),
            w.transform_vector3(Vec3::Y).normalize_or_zero(),
            w.transform_vector3(Vec3::Z).normalize_or_zero(),
        ]
    } else {
        [Vec3::X, Vec3::Y, Vec3::Z]
    };
    Some(GizmoInfo { origin, size, axes })
}

fn ring(origin: Vec3, axis: Vec3, r: f32) -> Vec<Vec3> {
    let helper = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let u = axis.cross(helper).normalize_or_zero();
    let v = axis.cross(u);
    (0..=48)
        .map(|k| {
            let t = k as f32 / 48.0 * TAU;
            origin + (u * t.cos() + v * t.sin()) * r
        })
        .collect()
}

fn hit_axis(view: &View, tool: Tool, g: &GizmoInfo, mouse: Pos2) -> Option<usize> {
    let mut best: (f32, Option<usize>) = (10.0, None);
    for i in 0..3 {
        let mut d = f32::MAX;
        if tool == Tool::Rotate {
            let pts = ring(g.origin, g.axes[i], g.size * 0.9);
            for w in pts.windows(2) {
                if let Some([a, b]) = view.line(w[0], w[1]) {
                    d = d.min(dist_seg(mouse, a, b));
                }
            }
        } else if let Some([a, b]) = view.line(g.origin, g.origin + g.axes[i] * g.size) {
            d = dist_seg(mouse, a, b);
        }
        if d < best.0 {
            best = (d, Some(i));
        }
    }
    best.1
}

fn apply_gizmo(gs: &mut GameState, view: &View, g: &GizmoInfo, delta: Vec2, shift: bool) {
    let Some(sel) = gs.selected else { return };
    let Some(drag) = gs.gizmo.clone() else { return };
    let axis = drag.axis;
    let tool = gs.tool;
    let snap = gs.snap;

    let amount = if tool == Tool::Rotate {
        (delta.x - delta.y) * 0.5
    } else {
        match (view.point(g.origin), view.point(g.origin + g.axes[axis] * g.size)) {
            (Some(a), Some(b)) => {
                let s = b - a;
                let l2 = s.length_sq();
                if l2 < 1.0 { 0.0 } else { delta.dot(s) / l2 * g.size }
            }
            _ => 0.0,
        }
    };
    let acc = drag.acc + amount;
    if let Some(d) = gs.gizmo.as_mut() {
        d.acc = acc;
    }

    let parent_inv = gs
        .scene
        .get(sel)
        .and_then(|e| e.parent)
        .map(|p| gs.scene.world_matrix(p).inverse())
        .unwrap_or(Mat4::IDENTITY);

    let Some(e) = gs.scene.get_mut(sel) else { return };
    match tool {
        Tool::Move => {
            let s = if snap { (acc / 0.5).round() * 0.5 } else { acc };
            e.pos = drag.start_pos + parent_inv.transform_vector3(g.axes[axis] * s);
        }
        Tool::Rotate => {
            let s = if snap { (acc / 15.0).round() * 15.0 } else { acc };
            e.rot = drag.start_rot;
            e.rot[axis] = drag.start_rot[axis] + s;
        }
        Tool::Scale => {
            let mut f = 1.0 + acc / g.size;
            if snap {
                f = (f / 0.1).round() * 0.1;
            }
            e.scale = drag.start_scale;
            if shift {
                e.scale = drag.start_scale * f;
            } else {
                e.scale[axis] = drag.start_scale[axis] * f;
            }
            e.scale = e.scale.max(Vec3::splat(0.01));
        }
        Tool::Select => {}
    }
}

fn draw_gizmo(p: &Painter, gs: &GameState, v: &View, g: &GizmoInfo, hover: Option<usize>) {
    for i in 0..3 {
        let active = hover == Some(i) || gs.gizmo.as_ref().map_or(false, |d| d.axis == i);
        let col = if active { HOT } else { AXIS_COLORS[i] };
        match gs.tool {
            Tool::Rotate => {
                let pts = ring(g.origin, g.axes[i], g.size * 0.9);
                for w in pts.windows(2) {
                    if let Some(l) = v.line(w[0], w[1]) {
                        p.line_segment(l, Stroke::new(if active { 3.5 } else { 2.0 }, col));
                    }
                }
            }
            Tool::Move | Tool::Scale => {
                let tip = g.origin + g.axes[i] * g.size;
                if let Some([a, b]) = v.line(g.origin, tip) {
                    p.line_segment([a, b], Stroke::new(if active { 4.0 } else { 3.0 }, col));
                    if gs.tool == Tool::Move {
                        let dir = b - a;
                        if dir.length() > 1.0 {
                            let dir = dir.normalized();
                            let perp = vec2(-dir.y, dir.x);
                            let base = b - dir * 10.0;
                            p.add(Shape::convex_polygon(
                                vec![b + dir * 6.0, base + perp * 5.0, base - perp * 5.0],
                                col,
                                Stroke::NONE,
                            ));
                        }
                    } else {
                        p.rect_filled(Rect::from_center_size(b, vec2(10.0, 10.0)), 1.0, col);
                    }
                }
            }
            Tool::Select => {}
        }
    }
    if let Some(o) = v.point(g.origin) {
        p.circle_filled(o, 4.0, Color32::WHITE);
    }
}



fn draw_helpers(p: &Painter, gs: &GameState, v: &View) {
    for e in &gs.scene.entities {
        if !gs.scene.is_visible(e.id) {
            continue;
        }
        let w = gs.scene.world_matrix(e.id);
        let pos = w.transform_point3(Vec3::ZERO);
        let selected = gs.selected == Some(e.id);
        let col = if selected { SELECT } else { Color32::from_rgba_unmultiplied(220, 220, 220, 190) };

        for c in &e.components {
            match c {
                Component::Camera { fov } => {
                    let d = 1.2;
                    let hh = (fov.to_radians() * 0.5).tan() * d;
                    let hw = hh * 1.7778;
                    let corners: Vec<Vec3> = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)]
                        .iter()
                        .map(|(x, y)| w.transform_point3(Vec3::new(*x, *y, -d)))
                        .collect();
                    for i in 0..4 {
                        if let Some(l) = v.line(pos, corners[i]) {
                            p.line_segment(l, Stroke::new(1.0, col));
                        }
                        if let Some(l) = v.line(corners[i], corners[(i + 1) % 4]) {
                            p.line_segment(l, Stroke::new(1.0, col));
                        }
                    }
                    if let Some(s) = v.point(pos) {
                        p.text(s, Align2::CENTER_CENTER, "📷", FontId::proportional(20.0), col);
                    }
                }
                Component::Light { kind, .. } => {
                    if *kind == LightKind::Directional {
                        let dir = w.transform_vector3(Vec3::NEG_Z).normalize_or_zero();
                        if let Some(l) = v.line(pos, pos + dir * 1.8) {
                            p.line_segment(l, Stroke::new(1.5, Color32::from_rgb(255, 225, 120)));
                            p.circle_filled(l[1], 3.0, Color32::from_rgb(255, 225, 120));
                        }
                    }
                    if let Some(s) = v.point(pos) {
                        p.text(s, Align2::CENTER_CENTER, "💡", FontId::proportional(20.0), col);
                    }
                }
                _ => {}
            }
        }
    }
}

fn draw_axis_widget(p: &Painter, v: &View, rect: Rect) {
    let center = pos2(rect.right() - 54.0, rect.top() + 54.0);
    p.circle_filled(center, 38.0, Color32::from_black_alpha(50));
    let names = ["X", "Y", "Z"];
    let dirs = [Vec3::X, Vec3::Y, Vec3::Z];
    let mut items: Vec<(f32, usize, Vec2)> = (0..3)
        .map(|i| {
            let vv = v.view.transform_vector3(dirs[i]);
            (vv.z, i, vec2(vv.x, -vv.y) * 26.0)
        })
        .collect();
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, i, off) in items {
        let tip = center + off;
        p.line_segment([center, tip], Stroke::new(2.0, AXIS_COLORS[i]));
        p.circle_filled(tip, 8.0, AXIS_COLORS[i]);
        p.text(tip, Align2::CENTER_CENTER, names[i], FontId::proportional(11.0), Color32::BLACK);
    }
}

// =====================================================================
//  Entry point
// =====================================================================

pub fn show(ui: &mut egui::Ui, gs: &mut GameState, link: &mut crate::render::Link) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let playing = gs.mode != PlayMode::Edit;
    let (alt, shift, dt) = ui.input(|i| (i.modifiers.alt, i.modifiers.shift, i.stable_dt));
    gs.fps = 0.92 * gs.fps + 0.08 * (1.0 / dt.max(0.0001));
    let aspect = (rect.width() / rect.height().max(1.0)).max(0.1);

    // دوربین بازی (در حالت Play)
    let game_cam: Option<(Mat4, f32, Vec3)> = if playing {
        gs.scene.entities.iter().find_map(|e| {
            if !gs.scene.is_visible(e.id) {
                return None;
            }
            e.components.iter().find_map(|c| match c {
                Component::Camera { fov } => {
                    let w = gs.scene.world_matrix(e.id);
                    Some((w.inverse(), *fov, w.transform_point3(Vec3::ZERO)))
                }
                _ => None,
            })
        })
    } else {
        None
    };

    // ۱) حرکت دوربین ادیتور
    if game_cam.is_none() {
        let d = response.drag_delta();
        if response.dragged_by(PointerButton::Secondary) || (alt && response.dragged_by(PointerButton::Primary)) {
            gs.cam.orbit(d.x, d.y);
        }
        if response.dragged_by(PointerButton::Middle) {
            gs.cam.pan(d.x, d.y);
        }
        if response.hovered() {
            let sc = ui.input(|i| i.raw_scroll_delta.y);
            if sc != 0.0 {
                gs.cam.zoom(sc);
            }
        }
        if response.dragged_by(PointerButton::Secondary) {
            let (fw, bk, lf, rt, up, dn) = ui.input(|i| {
                (
                    i.key_down(Key::W),
                    i.key_down(Key::S),
                    i.key_down(Key::A),
                    i.key_down(Key::D),
                    i.key_down(Key::E),
                    i.key_down(Key::Q),
                )
            });
            let f = (gs.cam.target - gs.cam.eye()).normalize();
            let r = f.cross(Vec3::Y).normalize();
            let speed = 6.0 * dt.min(0.05) * if shift { 3.0 } else { 1.0 };
            let mut mv = Vec3::ZERO;
            if fw { mv += f; }
            if bk { mv -= f; }
            if rt { mv += r; }
            if lf { mv -= r; }
            if up { mv += Vec3::Y; }
            if dn { mv -= Vec3::Y; }
            gs.cam.target += mv * speed;
            if mv != Vec3::ZERO {
                ui.ctx().request_repaint();
            }
        }
    }

    // ۲) ساخت View (برای Picking، Gizmo و آیکون‌ها)
    let (view_m, fov, eye) = match game_cam {
        Some((vm, fov, eye)) => (vm, fov, eye),
        None => (gs.cam.view(), gs.cam.fov, gs.cam.eye()),
    };
    let proj = Mat4::perspective_rh(fov.to_radians(), aspect, 0.1, 1000.0);
    let view = View { view: view_m, proj, vp: proj * view_m, eye, rect };

    // ۳) Gizmo و انتخاب
    let ginfo = if !playing || game_cam.is_none() { gizmo_info(gs, eye) } else { None };
    let mouse = response.hover_pos().or(response.interact_pointer_pos());
    let mut hover_axis: Option<usize> = None;
    if let (Some(g), Some(m)) = (&ginfo, mouse) {
        hover_axis = match &gs.gizmo {
            Some(d) => Some(d.axis),
            None => hit_axis(&view, gs.tool, g, m),
        };
    }
    if hover_axis.is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }

    if response.drag_started_by(PointerButton::Primary) && !alt {
        if let (Some(ax), Some(sel)) = (hover_axis, gs.selected) {
            if let Some(e) = gs.scene.get(sel) {
                gs.gizmo = Some(GizmoDrag {
                    axis: ax,
                    start_pos: e.pos,
                    start_rot: e.rot,
                    start_scale: e.scale,
                    acc: 0.0,
                });
            }
        }
    }
    if gs.gizmo.is_some() {
        if response.dragged_by(PointerButton::Primary) {
            if let Some(g) = &ginfo {
                apply_gizmo(gs, &view, g, response.drag_delta(), shift);
            }
        }
        if response.drag_released() || !response.dragged() {
            gs.gizmo = None;
        }
    }

    if response.clicked_by(PointerButton::Primary) && hover_axis.is_none() && game_cam.is_none() {
        if let Some(pos) = response.interact_pointer_pos() {
            gs.selected = pick(gs, &view, pos);
        }
    }

    // ۴) صحنه روی GPU رندر می‌شود؛ اینجا فقط درخواست را می‌سازیم و تصویر را نمایش می‌دهیم
    let ppp = ui.ctx().pixels_per_point();
    let scale = link.settings.scale.clamp(0.5, 1.0);
    let px = [
        ((rect.width() * ppp * scale).round() as u32).max(1),
        ((rect.height() * ppp * scale).round() as u32).max(1),
    ];
    gs.stat_faces = link.tris as usize;
    crate::render::fill_request(link, &*gs, view_m, fov, aspect, eye, px, playing);
    painter.image(
        link.tex,
        rect,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );

    // آیکون‌ها و Gizmo روی تصویر کشیده می‌شوند
    if !playing {
        draw_helpers(&painter, &*gs, &view);
        if let Some(g) = &ginfo {
            draw_gizmo(&painter, &*gs, &view, g, hover_axis);
        }
    }

    // ۵) Overlay
    draw_axis_widget(&painter, &view, rect);
    let tag = FontId::proportional(12.0);
    let label_col = Color32::from_rgba_unmultiplied(230, 230, 230, 200);
    painter.text(
        rect.left_top() + vec2(10.0, 8.0),
        Align2::LEFT_TOP,
        format!(
            "Perspective  •  {}  •  {}",
            gs.shading.label(),
            if game_cam.is_some() { "Game Camera" } else { "Scene Camera" }
        ),
        tag.clone(),
        label_col,
    );
    painter.text(
        rect.left_bottom() + vec2(10.0, -8.0),
        Align2::LEFT_BOTTOM,
        format!(
            "{} objects  •  {} tris  •  {:.0} FPS   |   RMB: orbit · MMB: pan · Wheel: zoom · RMB+WASD: fly · F: focus",
            gs.scene.entities.len(),
            gs.stat_faces,
            gs.fps
        ),
        tag.clone(),
        label_col,
    );
    if playing {
        painter.rect_stroke(rect.shrink(1.5), 0.0, Stroke::new(3.0_f32, Color32::from_rgb(70, 160, 255)));
        let t = if gs.mode == PlayMode::Playing {
            format!("▶ PLAYING  {:.1}s", gs.time)
        } else {
            "⏸ PAUSED".to_string()
        };
        painter.text(
            pos2(rect.center().x, rect.top() + 12.0),
            Align2::CENTER_TOP,
            t,
            FontId::proportional(15.0),
            Color32::from_rgb(120, 190, 255),
        );
    }
}