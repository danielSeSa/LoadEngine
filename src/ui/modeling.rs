use std::collections::HashSet;

use egui::{
    pos2, vec2, Align, Align2, Color32, Event, FontId, Key, Modifiers, Painter, PointerButton,
    Pos2, Rect, RichText, Sense, Shape, Stroke,
};
use glam::{Mat4, Vec2, Vec3};

use crate::game::{EntityId, GameState};
use crate::modeling::mesh::{EditMesh, NONE};
use crate::modeling::{Cam, Cmd, Constraint, ModalKind, ModelState, PrimKind, SelMode};
use crate::state::{AppState, Workspace};

const AXIS_COL: [Color32; 3] = [
    Color32::from_rgb(235, 75, 75),
    Color32::from_rgb(110, 205, 85),
    Color32::from_rgb(75, 125, 245),
];
const ORANGE: Color32 = Color32::from_rgb(255, 150, 40);
const CYAN: Color32 = Color32::from_rgb(90, 200, 255);

fn v2(p: Pos2) -> Vec2 {
    Vec2::new(p.x, p.y)
}
fn p2(v: Vec2) -> Pos2 {
    pos2(v.x, v.y)
}

// =====================================================================
//  Entry
// =====================================================================

pub fn draw(ctx: &egui::Context, app: &mut AppState) {
    let gs = &mut app.game;
    let ms = &mut app.modeling;
    let mut leave = toolbar(ctx, ms);
    status_bar(ctx, ms);
    left_panel(ctx, ms, gs);
    right_panel(ctx, ms, gs);
    egui::CentralPanel::default()
        .frame(egui::Frame::none())
        .show(ctx, |ui| {
            if viewport(ui, ms, &*gs) {
                leave = true;
            }
        });
    ms.sync(gs);
    if leave {
        app.active_workspace = Workspace::GameEngine;
    }
}

// =====================================================================
//  Toolbar / Status
// =====================================================================

fn toolbar(ctx: &egui::Context, ms: &mut ModelState) -> bool {
    let mut leave = false;
    egui::TopBottomPanel::top("model_toolbar").show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            if ui.button("◀ Game Engine").on_hover_text("Tab").clicked() {
                leave = true;
            }
            let label = if ms.target.is_some() {
                format!("🔗 {}", ms.target_name)
            } else {
                "Scratch mesh".to_string()
            };
            ui.label(RichText::new(label).strong().color(Color32::from_rgb(255, 190, 80)));
            ui.separator();
            for (m, label, tip) in [
                (SelMode::Vertex, "Vertex", "1"),
                (SelMode::Edge, "Edge", "2"),
                (SelMode::Face, "Face", "3"),
            ] {
                if ui.selectable_label(ms.mode == m, label).on_hover_text(tip).clicked() {
                    ms.set_mode(m);
                }
            }
            ui.separator();
            ui.menu_button("➕ Add", |ui| {
                for (k, n) in [
                    (PrimKind::Cube, "Cube"),
                    (PrimKind::Grid, "Grid"),
                    (PrimKind::Sphere, "UV Sphere"),
                    (PrimKind::Cylinder, "Cylinder"),
                    (PrimKind::Cone, "Cone"),
                    (PrimKind::Torus, "Torus"),
                ] {
                    if ui.button(n).clicked() {
                        ms.add_primitive(k);
                        ui.close_menu();
                    }
                }
            });
            ui.toggle_value(&mut ms.solid, "Solid");
            ui.toggle_value(&mut ms.show_edges, "Edges");
            ui.toggle_value(&mut ms.xray, "X-Ray");
            ui.toggle_value(&mut ms.smooth_shade, "Smooth");
            ui.separator();
            if ui.button("Front").clicked() {
                ms.cam.yaw = 0.0;
                ms.cam.pitch = 0.0;
            }
            if ui.button("Right").clicked() {
                ms.cam.yaw = std::f32::consts::FRAC_PI_2;
                ms.cam.pitch = 0.0;
            }
            if ui.button("Top").clicked() {
                ms.cam.pitch = 1.5;
            }
            ui.toggle_value(&mut ms.ortho, "Ortho").on_hover_text("5");
            if ui.button("Frame").on_hover_text("Home").clicked() {
                ms.frame_selected();
            }
            ui.separator();
            if ui.add_enabled(!ms.undo.is_empty(), egui::Button::new("↶")).on_hover_text("Ctrl+Z").clicked() {
                ms.undo();
            }
            if ui.add_enabled(!ms.redo.is_empty(), egui::Button::new("↷")).on_hover_text("Ctrl+Y").clicked() {
                ms.redo();
            }
        });
    });
    leave
}

fn status_bar(ctx: &egui::Context, ms: &mut ModelState) {
    egui::TopBottomPanel::bottom("model_status").exact_height(24.0).show(ctx, |ui| {
        ui.horizontal_centered(|ui| {
            let m = &ms.mesh;
            ui.label(format!(
                "Verts {}/{}  |  Edges {}  |  Faces {}/{}  |  Tris {}",
                m.sel_v.len(),
                m.verts.len(),
                m.edges.len(),
                m.sel_f.len(),
                m.faces.len(),
                m.tri_count()
            ));
            if m.sel_v.len() == 2 {
                let mut it = m.sel_v.iter();
                if let (Some(&a), Some(&b)) = (it.next(), it.next()) {
                    let d = (m.verts[a as usize] - m.verts[b as usize]).length();
                    ui.separator();
                    ui.label(format!("Distance {d:.4}"));
                }
            }
            if let Some(e) = &ms.eval {
                ui.separator();
                ui.label(format!("Modifiers → {} faces", e.faces.len()));
            }
            ui.separator();
            ui.label(RichText::new(&ms.status).color(Color32::from_rgb(120, 190, 255)));
        });
    });
}

// =====================================================================
//  Left panel (tools)
// =====================================================================

fn left_panel(ctx: &egui::Context, ms: &mut ModelState, gs: &mut GameState) {
    egui::SidePanel::left("model_tools").default_width(205.0).show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::CollapsingHeader::new("➕ Add").default_open(true).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (k, n) in [
                        (PrimKind::Cube, "Cube"),
                        (PrimKind::Grid, "Grid"),
                        (PrimKind::Sphere, "Sphere"),
                        (PrimKind::Cylinder, "Cylinder"),
                        (PrimKind::Cone, "Cone"),
                        (PrimKind::Torus, "Torus"),
                    ] {
                        if ui.button(n).clicked() {
                            ms.add_primitive(k);
                        }
                    }
                });
                ui.add(egui::Slider::new(&mut ms.add_size, 0.1..=10.0).text("Size"));
                ui.checkbox(&mut ms.grab_on_add, "Grab after add");
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button("Cursor → Selected").clicked() {
                        ms.cursor_to_selected();
                    }
                    if ui.small_button("Cursor → Origin").clicked() {
                        ms.cursor = Vec3::ZERO;
                    }
                });
                if ui.button("🗑 Clear mesh").clicked() {
                    ms.clear_mesh();
                }
            });

            egui::CollapsingHeader::new("🖱 Select").default_open(true).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.small_button("All (A)").clicked() {
                        ms.select_all();
                    }
                    if ui.small_button("None").clicked() {
                        ms.select_none();
                    }
                    if ui.small_button("Invert").clicked() {
                        ms.select_invert();
                    }
                    if ui.small_button("Linked (L)").clicked() {
                        ms.select_linked();
                    }
                    if ui.small_button("More").clicked() {
                        ms.select_more();
                    }
                    if ui.small_button("Less").clicked() {
                        ms.select_less();
                    }
                });
                ui.weak("Alt+Click: edge loop");
            });

            egui::CollapsingHeader::new("✥ Transform").default_open(true).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Grab G").clicked() {
                        ms.cmd = Some(Cmd::Grab);
                    }
                    if ui.button("Rotate R").clicked() {
                        ms.cmd = Some(Cmd::Rotate);
                    }
                    if ui.button("Scale S").clicked() {
                        ms.cmd = Some(Cmd::Scale);
                    }
                });
                ui.checkbox(&mut ms.proportional, "Proportional (O)");
                if ms.proportional {
                    ui.add(egui::Slider::new(&mut ms.prop_radius, 0.05..=20.0).text("Radius"));
                }
                ui.checkbox(&mut ms.snap, "Snap (or hold Ctrl)");
            });

            egui::CollapsingHeader::new("🛠 Mesh").default_open(true).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Extrude E").clicked() {
                        ms.cmd = Some(Cmd::Extrude);
                    }
                    if ui.button("Inset I").clicked() {
                        ms.cmd = Some(Cmd::Inset);
                    }
                    if ui.button("Duplicate ⇧D").clicked() {
                        ms.cmd = Some(Cmd::Duplicate);
                    }
                    if ui.button("Fill F").clicked() {
                        ms.op_fill();
                    }
                    if ui.button("Dissolve ^X").clicked() {
                        ms.op_dissolve();
                    }
                    if ui.button("Delete X").clicked() {
                        ms.op_delete();
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Loop Cut ^R").clicked() {
                        ms.op_loop_cut();
                    }
                    ui.add(egui::DragValue::new(&mut ms.cuts).clamp_range(1..=16));
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Subdivide").clicked() {
                        ms.op_subdivide();
                    }
                    if ui.button("Poke").clicked() {
                        ms.op_poke();
                    }
                    if ui.button("Triangulate").clicked() {
                        ms.op_triangulate();
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Solidify").clicked() {
                        ms.op_solidify();
                    }
                    ui.add(egui::DragValue::new(&mut ms.solidify_t).speed(0.005).max_decimals(3));
                });
                ui.horizontal(|ui| {
                    if ui.button("To Sphere").clicked() {
                        ms.op_to_sphere();
                    }
                    ui.add(egui::DragValue::new(&mut ms.sphere_f).speed(0.01).clamp_range(0.0..=1.0));
                });
            });

            egui::CollapsingHeader::new("🔗 Merge / Normals").default_open(false).show(ui, |ui| {
                if ui.button("Merge at Center (M)").clicked() {
                    ms.op_merge_center();
                }
                ui.horizontal(|ui| {
                    if ui.button("By Distance").clicked() {
                        ms.op_weld();
                    }
                    ui.add(egui::DragValue::new(&mut ms.weld_eps).speed(0.0001).max_decimals(5));
                });
                ui.horizontal(|ui| {
                    if ui.button("Flip").clicked() {
                        ms.op_flip();
                    }
                    if ui.button("Recalc ⇧N").clicked() {
                        ms.op_recalc();
                    }
                });
                ui.add(egui::Slider::new(&mut ms.smooth_iter, 1..=10).text("Iter"));
                ui.add(egui::Slider::new(&mut ms.smooth_amt, 0.0..=1.0).text("Amount"));
                if ui.button("Smooth Vertices").clicked() {
                    ms.op_smooth();
                }
                ui.horizontal(|ui| {
                    ui.label("Mirror (apply):");
                    for (i, n) in ["X", "Y", "Z"].iter().enumerate() {
                        if ui.small_button(*n).clicked() {
                            ms.op_mirror(i);
                        }
                    }
                });
            });

            egui::CollapsingHeader::new("🎮 Game Object").default_open(true).show(ui, |ui| {
                if ui.button("Separate selection → new object").clicked() {
                    ms.op_separate(gs);
                }
                ui.horizontal(|ui| {
                    if ui.small_button("Origin → Center").clicked() {
                        ms.op_recenter();
                    }
                    if ui.small_button("Origin → Bottom").clicked() {
                        ms.op_origin_bottom();
                    }
                });
            });
        });
    });
}

// =====================================================================
//  Right panel (object / modifiers / overlays)
// =====================================================================

fn right_panel(ctx: &egui::Context, ms: &mut ModelState, gs: &mut GameState) {
    egui::SidePanel::right("model_props").default_width(240.0).show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.strong("🔗 Object");
            let names: Vec<(EntityId, String)> =
                gs.scene.entities.iter().map(|e| (e.id, e.name.clone())).collect();
            let current = if ms.target.is_some() {
                ms.target_name.clone()
            } else {
                "(scratch mesh)".to_string()
            };
            let mut pick: Option<EntityId> = None;
            egui::ComboBox::from_id_source("model_target")
                .selected_text(current)
                .width(190.0)
                .show_ui(ui, |ui| {
                    for (id, n) in &names {
                        if ui.selectable_label(ms.target == Some(*id), n.as_str()).clicked() {
                            pick = Some(*id);
                        }
                    }
                });
            if let Some(id) = pick {
                ms.load_entity(gs, id);
            }
            ui.checkbox(&mut ms.live, "Live sync to Game Engine");
            ui.horizontal(|ui| {
                if ui.button("Send as new").clicked() {
                    ms.send_as_new(gs);
                }
                if ui.button("Detach").clicked() {
                    ms.detach();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Color");
                if ui.color_edit_button_rgb(&mut ms.color).changed() {
                    ms.needs_sync = true;
                }
            });
            ui.separator();

            ui.strong("🧩 Modifiers");
            ui.horizontal(|ui| {
                ui.label("Mirror");
                for (i, n) in ["X", "Y", "Z"].iter().enumerate() {
                    if ui.checkbox(&mut ms.mirror[i], *n).changed() {
                        ms.needs_sync = true;
                    }
                }
            });
            if ui.add(egui::Slider::new(&mut ms.subsurf, 0..=3).text("Subdivision")).changed() {
                ms.needs_sync = true;
            }
            if ui.button("Apply modifiers").clicked() {
                ms.apply_modifiers();
            }
            ui.separator();

            ui.strong("👁 Overlays");
            ui.checkbox(&mut ms.material, "Material color");
            ui.checkbox(&mut ms.show_context, "Show other scene objects");
            ui.checkbox(&mut ms.cull, "Backface culling");
            ui.checkbox(&mut ms.grid, "Grid");
            ui.checkbox(&mut ms.normals, "Face normals");
            ui.separator();

            ui.strong("📐 Selection median");
            let m0 = ms.mesh.median();
            let mut m1 = m0;
            let mut start = false;
            ui.horizontal(|ui| {
                for (k, l) in ["X", "Y", "Z"].iter().enumerate() {
                    ui.colored_label(AXIS_COL[k], *l);
                    let r = ui.add(egui::DragValue::new(&mut m1[k]).speed(0.01).max_decimals(3));
                    if r.drag_started() || r.gained_focus() {
                        start = true;
                    }
                }
            });
            if ms.mesh.sel_v.is_empty() {
                ui.weak("Nothing selected");
            } else {
                if start {
                    ms.push_undo();
                }
                if m1 != m0 {
                    ms.translate_selection(m1 - m0);
                }
            }
            if let Some((lo, hi)) = ms.mesh.bounds() {
                let d = hi - lo;
                ui.weak(format!("Dimensions {:.2} × {:.2} × {:.2}", d.x, d.y, d.z));
            }
            ui.separator();

            ui.strong("💾 OBJ");
            ui.add(egui::TextEdit::singleline(&mut ms.obj_path).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                if ui.button("Export").clicked() {
                    ms.export_obj();
                }
                if ui.button("Import").clicked() {
                    ms.import_obj();
                }
            });
            ui.separator();

            ui.strong("📟 Log");
            egui::ScrollArea::vertical()
                .id_source("model_log")
                .max_height(140.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for l in &ms.log {
                        ui.label(RichText::new(l).monospace().size(11.0));
                    }
                });
        });
    });
}

// =====================================================================
//  Viewport
// =====================================================================

fn run_cmd(ms: &mut ModelState, c: Cmd, mouse: Vec2) {
    match c {
        Cmd::Grab => {
            ms.begin_transform(ModalKind::Grab, mouse);
        }
        Cmd::Rotate => {
            ms.begin_transform(ModalKind::Rotate, mouse);
        }
        Cmd::Scale => {
            ms.begin_transform(ModalKind::Scale, mouse);
        }
        Cmd::Extrude => ms.op_extrude(mouse),
        Cmd::Inset => ms.op_inset(mouse),
        Cmd::Duplicate => ms.op_duplicate(mouse),
    }
}

fn set_view(ms: &mut ModelState, axis: usize, sign: f32) {
    use std::f32::consts::{FRAC_PI_2, PI};
    match (axis, sign > 0.0) {
        (0, true) => {
            ms.cam.yaw = FRAC_PI_2;
            ms.cam.pitch = 0.0;
        }
        (0, false) => {
            ms.cam.yaw = -FRAC_PI_2;
            ms.cam.pitch = 0.0;
        }
        (1, true) => ms.cam.pitch = 1.5,
        (1, false) => ms.cam.pitch = -1.5,
        (2, true) => {
            ms.cam.yaw = 0.0;
            ms.cam.pitch = 0.0;
        }
        _ => {
            ms.cam.yaw = PI;
            ms.cam.pitch = 0.0;
        }
    }
}

struct NavTip {
    pos: Pos2,
    r: f32,
    axis: usize,
    sign: f32,
    depth: f32,
}

fn nav_center(rect: Rect) -> Pos2 {
    pos2(rect.right() - 62.0, rect.top() + 62.0)
}

fn nav_tips(cam: &Cam, rect: Rect) -> Vec<NavTip> {
    let c = nav_center(rect);
    let mut v = Vec::with_capacity(6);
    for axis in 0..3 {
        for sign in [1.0_f32, -1.0] {
            let mut d = Vec3::ZERO;
            d[axis] = sign;
            let t = cam.view.transform_vector3(d);
            v.push(NavTip {
                pos: c + vec2(t.x, -t.y) * 32.0,
                r: if sign > 0.0 { 9.0 } else { 6.0 },
                axis,
                sign,
                depth: t.z,
            });
        }
    }
    v
}

/// خروجی true یعنی کاربر Tab زده و باید به Game Engine برگردیم
fn viewport(ui: &mut egui::Ui, ms: &mut ModelState, gs: &GameState) -> bool {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let ctx = ui.ctx().clone();
    let (alt, shift, ctrl) = ui.input(|i| (i.modifiers.alt, i.modifiers.shift, i.modifiers.command));
    let mut leave = false;

    // ناوبری دوربین
    if ms.modal.is_none() {
        let d = resp.drag_delta();
        if resp.dragged_by(PointerButton::Secondary) || (alt && resp.dragged_by(PointerButton::Primary)) {
            ms.cam.orbit(d.x, d.y);
        }
        if resp.dragged_by(PointerButton::Middle) {
            ms.cam.pan(d.x, d.y);
        }
    }
    if resp.hovered() {
        let sc = ui.input(|i| i.raw_scroll_delta.y);
        if sc != 0.0 {
            if ms.modal.is_some() {
                ms.modal_scroll(sc);
            } else {
                ms.cam.zoom(sc);
            }
        }
    }

    let aspect = (rect.width() / rect.height().max(1.0)).max(0.1);
    let view = ms.cam.view();
    let fov = ms.cam.fov;
    let (proj, ortho_h) = if ms.ortho {
        let h = ms.cam.dist * (fov.to_radians() * 0.5).tan();
        (Mat4::orthographic_rh(-h * aspect, h * aspect, -h, h, 0.01, 4000.0), h)
    } else {
        (Mat4::perspective_rh(fov.to_radians(), aspect, 0.05, 1000.0), 0.0)
    };
    let cam = Cam::new(
        view,
        proj,
        ms.cam.eye(),
        v2(rect.min),
        Vec2::new(rect.width(), rect.height()),
        fov,
        ortho_h,
    );
    let mouse = ui
        .input(|i| i.pointer.latest_pos())
        .map(v2)
        .unwrap_or_else(|| v2(rect.center()));

    if let Some(c) = ms.cmd.take() {
        run_cmd(ms, c, mouse);
    }
    ms.prepare(&cam);

    let typing = ctx.wants_keyboard_input();
    if ms.modal.is_some() {
        modal_input(ui, ms, &cam, mouse, ctrl, &resp);
    } else {
        let mut nav_hit = false;
        if resp.clicked_by(PointerButton::Primary) {
            if let Some(p) = resp.interact_pointer_pos() {
                for t in nav_tips(&cam, rect) {
                    if (t.pos - p).length() <= t.r + 2.0 {
                        set_view(ms, t.axis, t.sign);
                        nav_hit = true;
                        break;
                    }
                }
            }
        }
        if !typing {
            if resp.hovered() && hotkeys(ui, ms, mouse) {
                leave = true;
            }
            if resp.drag_started_by(PointerButton::Primary) && !alt && !nav_hit {
                ms.box_start = resp.interact_pointer_pos().map(v2);
            }
            if resp.clicked_by(PointerButton::Primary) && !nav_hit {
                if let Some(p) = resp.interact_pointer_pos() {
                    if alt {
                        ms.pick_loop(v2(p), shift);
                    } else {
                        ms.pick(&cam, v2(p), shift);
                    }
                }
            }
            if resp.drag_released_by(PointerButton::Primary) {
                if let Some(s) = ms.box_start.take() {
                    if (mouse - s).length() > 4.0 {
                        ms.box_select(s.min(mouse), s.max(mouse), shift);
                    }
                }
            }
            if !resp.dragged_by(PointerButton::Primary) {
                ms.box_start = None;
            }
        }
    }

    // اگر مش در همین فریم تغییر کرد، کش را دوباره بساز
    if ms.needs_sync || !ms.cache_ok() {
        ms.prepare(&cam);
    }

    if ms.modal.is_none() {
        resp.context_menu(|ui| context_items(ui, ms));
    }

    draw_scene(&painter, ms, gs, &cam);
    draw_overlay(&painter, ms, &cam, rect, mouse);
    draw_nav(&painter, &cam, rect);
    leave
}

fn context_items(ui: &mut egui::Ui, ms: &mut ModelState) {
    let item = |ui: &mut egui::Ui, label: &str| -> bool {
        let c = ui.button(label).clicked();
        if c {
            ui.close_menu();
        }
        c
    };
    if item(ui, "Extrude  (E)") {
        ms.cmd = Some(Cmd::Extrude);
    }
    if item(ui, "Inset  (I)") {
        ms.cmd = Some(Cmd::Inset);
    }
    if item(ui, "Duplicate  (Shift+D)") {
        ms.cmd = Some(Cmd::Duplicate);
    }
    if item(ui, "Loop Cut  (Ctrl+R)") {
        ms.op_loop_cut();
    }
    if item(ui, "Fill  (F)") {
        ms.op_fill();
    }
    if item(ui, "Dissolve Edges  (Ctrl+X)") {
        ms.op_dissolve();
    }
    if item(ui, "Delete  (X)") {
        ms.op_delete();
    }
    ui.separator();
    if item(ui, "Merge at Center  (M)") {
        ms.op_merge_center();
    }
    if item(ui, "Subdivide") {
        ms.op_subdivide();
    }
    if item(ui, "Flip Normals") {
        ms.op_flip();
    }
    if item(ui, "Select Linked  (L)") {
        ms.select_linked();
    }
    if item(ui, "Select All  (A)") {
        ms.select_all();
    }
}

fn modal_input(ui: &egui::Ui, ms: &mut ModelState, cam: &Cam, mouse: Vec2, ctrl: bool, resp: &egui::Response) {
    ms.update_modal(cam, mouse, ctrl);

    let typed: Vec<String> = ui.input(|i| {
        i.events
            .iter()
            .filter_map(|e| if let Event::Text(t) = e { Some(t.clone()) } else { None })
            .collect()
    });
    for t in &typed {
        ms.modal_text(t);
    }
    let (enter, esc, back, x, y, z, n) = ui.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Escape),
            i.consume_key(Modifiers::NONE, Key::Backspace),
            i.consume_key(Modifiers::NONE, Key::X),
            i.consume_key(Modifiers::NONE, Key::Y),
            i.consume_key(Modifiers::NONE, Key::Z),
            i.consume_key(Modifiers::NONE, Key::N),
        )
    });
    if back {
        ms.modal_backspace();
    }
    if x {
        ms.toggle_constraint(Constraint::X);
    }
    if y {
        ms.toggle_constraint(Constraint::Y);
    }
    if z {
        ms.toggle_constraint(Constraint::Z);
    }
    if n {
        ms.toggle_constraint(Constraint::Normal);
    }
    if enter || resp.clicked_by(PointerButton::Primary) {
        ms.confirm_modal();
    } else if esc || resp.clicked_by(PointerButton::Secondary) {
        ms.cancel_modal();
    }
}

/// خروجی true یعنی Tab (بازگشت به Game Engine)
fn hotkeys(ui: &egui::Ui, ms: &mut ModelState, mouse: Vec2) -> bool {
    let hit = |m: Modifiers, k: Key| ui.input_mut(|i| i.consume_key(m, k));

    if hit(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || hit(Modifiers::COMMAND, Key::Y) {
        ms.redo();
    } else if hit(Modifiers::COMMAND, Key::Z) {
        ms.undo();
    } else if hit(Modifiers::COMMAND, Key::R) {
        ms.op_loop_cut();
    } else if hit(Modifiers::COMMAND, Key::I) {
        ms.select_invert();
    } else if hit(Modifiers::COMMAND, Key::X) {
        ms.op_dissolve();
    } else if hit(Modifiers::ALT, Key::A) {
        ms.select_none();
    } else if hit(Modifiers::ALT, Key::Z) {
        ms.xray = !ms.xray;
    } else if hit(Modifiers::SHIFT, Key::N) {
        ms.op_recalc();
    } else if hit(Modifiers::SHIFT, Key::D) {
        run_cmd(ms, Cmd::Duplicate, mouse);
    } else if hit(Modifiers::NONE, Key::Tab) {
        return true;
    } else if hit(Modifiers::NONE, Key::A) {
        ms.select_all();
    } else if hit(Modifiers::NONE, Key::L) {
        ms.select_linked();
    } else if hit(Modifiers::NONE, Key::Num1) {
        ms.set_mode(SelMode::Vertex);
    } else if hit(Modifiers::NONE, Key::Num2) {
        ms.set_mode(SelMode::Edge);
    } else if hit(Modifiers::NONE, Key::Num3) {
        ms.set_mode(SelMode::Face);
    } else if hit(Modifiers::NONE, Key::Num5) {
        ms.ortho = !ms.ortho;
    } else if hit(Modifiers::NONE, Key::Home) {
        ms.frame_selected();
    } else if hit(Modifiers::NONE, Key::O) {
        ms.proportional = !ms.proportional;
    } else if hit(Modifiers::NONE, Key::F) {
        ms.op_fill();
    } else if hit(Modifiers::NONE, Key::X) || hit(Modifiers::NONE, Key::Delete) {
        ms.op_delete();
    } else if hit(Modifiers::NONE, Key::M) {
        ms.op_merge_center();
    } else if hit(Modifiers::NONE, Key::G) {
        run_cmd(ms, Cmd::Grab, mouse);
    } else if hit(Modifiers::NONE, Key::R) {
        run_cmd(ms, Cmd::Rotate, mouse);
    } else if hit(Modifiers::NONE, Key::S) {
        run_cmd(ms, Cmd::Scale, mouse);
    } else if hit(Modifiers::NONE, Key::E) {
        run_cmd(ms, Cmd::Extrude, mouse);
    } else if hit(Modifiers::NONE, Key::I) {
        run_cmd(ms, Cmd::Inset, mouse);
    }
    false
}

// =====================================================================
//  Drawing
// =====================================================================

fn draw_background(p: &Painter, rect: Rect) {
    let top = Color32::from_rgb(62, 68, 82);
    let bot = Color32::from_rgb(30, 33, 40);
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bot);
    mesh.colored_vertex(rect.left_bottom(), bot);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
}

fn shade(base: Vec3, n: Vec3, v: Vec3, key: Vec3) -> Vec3 {
    let n = if n.dot(v) < 0.0 { -n } else { n };
    let head = n.dot(v).max(0.0);
    let k = n.dot(key).max(0.0);
    let hemi = n.y * 0.5 + 0.5;
    let h = (v + key).normalize_or_zero();
    let spec = n.dot(h).max(0.0).powf(28.0) * 0.16;
    base * (0.16 + 0.26 * hemi + 0.36 * head + 0.30 * k) + Vec3::splat(spec)
}

fn col3(c: Vec3, a: u8) -> Color32 {
    let c = c.clamp(Vec3::ZERO, Vec3::ONE);
    Color32::from_rgba_unmultiplied((c.x * 255.0) as u8, (c.y * 255.0) as u8, (c.z * 255.0) as u8, a)
}

struct SolidOpts {
    base: Vec3,
    key: Vec3,
    smooth: bool,
    cull: bool,
    alpha: u8,
}

/// رسم سطح سایه‌دار (همه‌ی وجه‌ها در یک egui::Mesh)
fn draw_solid(p: &Painter, m: &EditMesh, cam: &Cam, o: &SolidOpts, sel: Option<&HashSet<u32>>) {
    let nf = m.faces.len();
    if nf == 0 || nf > 150_000 {
        return;
    }
    let proj: Vec<Option<Vec2>> = m.verts.iter().map(|&v| cam.project(v)).collect();
    let mut fnrm: Vec<Vec3> = Vec::with_capacity(nf);
    let mut vn: Vec<Vec3> = if o.smooth { vec![Vec3::ZERO; m.verts.len()] } else { Vec::new() };
    let mut order: Vec<(f32, u32)> = Vec::with_capacity(nf);
    for fi in 0..nf {
        let n = m.face_normal(fi);
        let c = m.face_center(fi);
        fnrm.push(n);
        if o.smooth {
            for &v in &m.faces[fi] {
                vn[v as usize] += n;
            }
        }
        if !o.cull || n.dot(cam.to_eye(c)) > 0.0 {
            order.push((cam.depth(c), fi as u32));
        }
    }
    if o.smooth {
        for n in vn.iter_mut() {
            *n = n.normalize_or_zero();
        }
    }
    order.sort_unstable_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut mesh = egui::Mesh::default();
    let mut selm = egui::Mesh::default();
    let sel_col = Color32::from_rgba_unmultiplied(255, 150, 40, 100);
    for (_, fi) in order {
        let f = &m.faces[fi as usize];
        if f.len() < 3 || f.iter().any(|&v| proj[v as usize].is_none()) {
            continue;
        }
        let flat = if o.smooth {
            Color32::BLACK
        } else {
            let c = m.face_center(fi as usize);
            col3(shade(o.base, fnrm[fi as usize], cam.to_eye(c).normalize_or_zero(), o.key), o.alpha)
        };
        let b0 = mesh.vertices.len() as u32;
        for &v in f {
            let c = if o.smooth {
                let ve = cam.to_eye(m.verts[v as usize]).normalize_or_zero();
                col3(shade(o.base, vn[v as usize], ve, o.key), o.alpha)
            } else {
                flat
            };
            mesh.colored_vertex(p2(proj[v as usize].unwrap()), c);
        }
        for i in 1..f.len() as u32 - 1 {
            mesh.add_triangle(b0, b0 + i, b0 + i + 1);
        }
        if let Some(s) = sel {
            if s.contains(&fi) {
                let b1 = selm.vertices.len() as u32;
                for &v in f {
                    selm.colored_vertex(p2(proj[v as usize].unwrap()), sel_col);
                }
                for i in 1..f.len() as u32 - 1 {
                    selm.add_triangle(b1, b1 + i, b1 + i + 1);
                }
            }
        }
    }
    p.add(Shape::mesh(mesh));
    p.add(Shape::mesh(selm));
}

fn draw_sel_faces(p: &Painter, ms: &ModelState) {
    let mut selm = egui::Mesh::default();
    let c = Color32::from_rgba_unmultiplied(255, 150, 40, 90);
    for &fi in &ms.mesh.sel_f {
        let f = &ms.mesh.faces[fi as usize];
        if !ms.front[fi as usize] || f.iter().any(|&v| ms.proj[v as usize].is_none()) {
            continue;
        }
        let b = selm.vertices.len() as u32;
        for &v in f {
            selm.colored_vertex(p2(ms.proj[v as usize].unwrap()), c);
        }
        for i in 1..f.len() as u32 - 1 {
            selm.add_triangle(b, b + i, b + i + 1);
        }
    }
    p.add(Shape::mesh(selm));
}

fn draw_grid(p: &Painter, cam: &Cam) {
    let n = 10;
    let mut shapes = Vec::new();
    for i in -n..=n {
        let f = i as f32;
        let nf = n as f32;
        let base = Color32::from_rgba_unmultiplied(255, 255, 255, if i % 5 == 0 { 34 } else { 16 });
        let cz = if i == 0 { Color32::from_rgba_unmultiplied(75, 125, 245, 160) } else { base };
        let cx = if i == 0 { Color32::from_rgba_unmultiplied(235, 75, 75, 160) } else { base };
        if let Some([a, b]) = cam.segment(Vec3::new(f, 0.0, -nf), Vec3::new(f, 0.0, nf)) {
            shapes.push(Shape::line_segment([p2(a), p2(b)], Stroke::new(1.0_f32, cz)));
        }
        if let Some([a, b]) = cam.segment(Vec3::new(-nf, 0.0, f), Vec3::new(nf, 0.0, f)) {
            shapes.push(Shape::line_segment([p2(a), p2(b)], Stroke::new(1.0_f32, cx)));
        }
    }
    p.extend(shapes);
}

/// آبجکت‌های دیگر صحنه به‌صورت Wire کم‌رنگ (در فضای محلی آبجکت هدف)
fn draw_context(p: &Painter, gs: &GameState, ms: &ModelState, cam: &Cam) {
    let Some(t) = ms.target else { return };
    if gs.scene.get(t).is_none() {
        return;
    }
    let inv = gs.scene.world_matrix(t).inverse();
    let col = Color32::from_rgba_unmultiplied(150, 165, 190, 70);
    let mut shapes = Vec::new();
    let mut budget = 8000usize;
    for e in &gs.scene.entities {
        if e.id == t || !gs.scene.is_visible(e.id) {
            continue;
        }
        let Some((prim, _)) = e.mesh() else { continue };
        let mesh = gs.meshes.get(prim);
        let m = inv * gs.scene.world_matrix(e.id);
        for &(a, b) in &mesh.edges {
            if budget == 0 {
                break;
            }
            budget -= 1;
            let pa = m.transform_point3(mesh.verts[a]);
            let pb = m.transform_point3(mesh.verts[b]);
            if let Some([s0, s1]) = cam.segment(pa, pb) {
                shapes.push(Shape::line_segment([p2(s0), p2(s1)], Stroke::new(1.0_f32, col)));
            }
        }
    }
    p.extend(shapes);
}

fn draw_scene(p: &Painter, ms: &ModelState, gs: &GameState, cam: &Cam) {
    let rect = Rect::from_min_size(p2(cam.min), vec2(cam.size.x, cam.size.y));
    draw_background(p, rect);
    if ms.grid {
        draw_grid(p, cam);
    }
    if ms.show_context {
        draw_context(p, gs, ms, cam);
    }

    let m = &ms.mesh;
    let key = (-cam.right() * 0.45 + cam.up() * 0.65 - cam.forward() * 0.75).normalize_or_zero();
    let base = if ms.material { Vec3::from(ms.color) } else { Vec3::splat(0.78) };
    let opts = SolidOpts {
        base,
        key,
        smooth: ms.smooth_shade,
        cull: ms.cull && !ms.open && !ms.xray,
        alpha: if ms.xray { 110 } else { 255 },
    };

    let has_eval = ms.eval.is_some();
    if ms.solid {
        match &ms.eval {
            Some(ev) => {
                draw_solid(p, ev, cam, &opts, None);
                draw_sel_faces(p, ms);
            }
            None => draw_solid(p, m, cam, &opts, Some(&m.sel_f)),
        }
    }

    // یال‌ها
    let mut shapes: Vec<Shape> = Vec::with_capacity(m.edges.len());
    let edge_col = if ms.solid {
        Color32::from_rgba_unmultiplied(0, 0, 0, if has_eval { 200 } else { 150 })
    } else {
        Color32::from_rgb(170, 195, 230)
    };
    for e in &m.edges {
        let vis = ms.xray || ms.front[e.f0 as usize] || (e.f1 != NONE && ms.front[e.f1 as usize]);
        if !vis {
            continue;
        }
        let selected = m.sel_v.contains(&e.a) && m.sel_v.contains(&e.b);
        if !ms.show_edges && !selected {
            continue;
        }
        let Some([a, b]) = cam.segment(m.verts[e.a as usize], m.verts[e.b as usize]) else {
            continue;
        };
        let stroke = if selected { Stroke::new(2.5_f32, ORANGE) } else { Stroke::new(1.0_f32, edge_col) };
        shapes.push(Shape::line_segment([p2(a), p2(b)], stroke));
    }
    p.extend(shapes);

    // رأس‌ها و مرکز وجه‌ها
    let mut dots: Vec<Shape> = Vec::new();
    match ms.mode {
        SelMode::Vertex => {
            for (i, pt) in ms.proj.iter().enumerate() {
                let Some(pt) = pt else { continue };
                if !(ms.xray || ms.vis_v[i]) {
                    continue;
                }
                let sel = m.sel_v.contains(&(i as u32));
                let (r, c) = if sel { (3.5_f32, ORANGE) } else { (2.5_f32, Color32::from_rgb(20, 20, 24)) };
                dots.push(Shape::rect_filled(Rect::from_center_size(p2(*pt), vec2(r * 2.0, r * 2.0)), 0.0, c));
            }
        }
        SelMode::Face => {
            for fi in 0..m.faces.len() {
                if !ms.front[fi] {
                    continue;
                }
                let f = &m.faces[fi];
                let mut s = Vec2::ZERO;
                let mut ok = true;
                for &v in f {
                    match ms.proj[v as usize] {
                        Some(q) => s += q,
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    continue;
                }
                let sel = m.sel_f.contains(&(fi as u32));
                dots.push(Shape::circle_filled(
                    p2(s / f.len() as f32),
                    if sel { 3.5_f32 } else { 2.0_f32 },
                    if sel { ORANGE } else { Color32::from_rgb(20, 20, 24) },
                ));
            }
        }
        SelMode::Edge => {}
    }
    p.extend(dots);

    if ms.normals {
        let mut ns = Vec::new();
        for &fi in &m.sel_f {
            let fi = fi as usize;
            let c = ms.fcen[fi];
            if let Some([a, b]) = cam.segment(c, c + ms.fnorm[fi] * 0.4) {
                ns.push(Shape::line_segment([p2(a), p2(b)], Stroke::new(2.0_f32, CYAN)));
            }
        }
        p.extend(ns);
    }
}

fn draw_overlay(p: &Painter, ms: &ModelState, cam: &Cam, rect: Rect, mouse: Vec2) {
    // مبدأ آبجکت و Cursor سه‌بعدی
    if let Some(o) = cam.project(Vec3::ZERO) {
        p.circle_filled(p2(o), 3.0, ORANGE);
    }
    if let Some(c) = cam.project(ms.cursor) {
        let c = p2(c);
        p.circle_stroke(c, 8.0, Stroke::new(2.0_f32, Color32::from_rgb(235, 75, 75)));
        p.circle_stroke(c, 8.0, Stroke::new(1.0_f32, Color32::WHITE));
        p.line_segment([c - vec2(12.0, 0.0), c + vec2(12.0, 0.0)], Stroke::new(1.0_f32, Color32::WHITE));
        p.line_segment([c - vec2(0.0, 12.0), c + vec2(0.0, 12.0)], Stroke::new(1.0_f32, Color32::WHITE));
    }

    if let Some(m) = &ms.modal {
        let (axis, col) = match m.constraint {
            Constraint::X => (Some(Vec3::X), AXIS_COL[0]),
            Constraint::Y => (Some(Vec3::Y), AXIS_COL[1]),
            Constraint::Z => (Some(Vec3::Z), AXIS_COL[2]),
            Constraint::Normal => (Some(m.normal), CYAN),
            Constraint::Free => (None, Color32::WHITE),
        };
        if let Some(a) = axis {
            if let Some([s, e]) = cam.segment(m.pivot - a * 100.0, m.pivot + a * 100.0) {
                p.line_segment([p2(s), p2(e)], Stroke::new(1.5_f32, col));
            }
        }
        if let Some(pp) = cam.project(m.pivot) {
            p.circle_filled(p2(pp), 4.0, Color32::WHITE);
            if m.kind != ModalKind::Grab {
                p.line_segment([p2(pp), p2(mouse)], Stroke::new(1.0_f32, Color32::from_white_alpha(120)));
            }
            if ms.proportional && m.kind != ModalKind::Inset {
                let r = ms.prop_radius / cam.units_per_px(m.pivot).max(1e-6);
                p.circle_stroke(p2(pp), r, Stroke::new(1.0_f32, Color32::from_white_alpha(90)));
            }
        }
    }

    if let Some(s) = ms.box_start {
        let r = Rect::from_two_pos(p2(s), p2(mouse));
        p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18));
        p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::from_white_alpha(200)));
    }

    let label = Color32::from_rgba_unmultiplied(230, 230, 230, 200);
    let mode = match ms.mode {
        SelMode::Vertex => "Vertex",
        SelMode::Edge => "Edge",
        SelMode::Face => "Face",
    };
    p.text(
        rect.left_top() + vec2(10.0, 8.0),
        Align2::LEFT_TOP,
        format!(
            "Edit Mode • {mode}{}{}{}",
            if ms.ortho { " • Ortho" } else { " • Persp" },
            if ms.xray { " • X-Ray" } else { "" },
            if ms.proportional { " • Proportional" } else { "" },
        ),
        FontId::proportional(12.0),
        label,
    );
    p.text(
        rect.left_bottom() + vec2(10.0, -8.0),
        Align2::LEFT_BOTTOM,
        "LMB select/box · Alt+LMB loop · RMB drag orbit / click menu · MMB pan · G R S · E I ⇧D F · Tab back to scene",
        FontId::proportional(11.0),
        label,
    );
    let _ = Align::Min;
}

fn draw_nav(p: &Painter, cam: &Cam, rect: Rect) {
    let c = nav_center(rect);
    p.circle_filled(c, 46.0, Color32::from_black_alpha(45));
    let mut tips = nav_tips(cam, rect);
    tips.sort_by(|a, b| a.depth.partial_cmp(&b.depth).unwrap_or(std::cmp::Ordering::Equal));
    let names = ["X", "Y", "Z"];
    for t in tips {
        let base = AXIS_COL[t.axis];
        if t.sign > 0.0 {
            p.line_segment([c, t.pos], Stroke::new(2.0_f32, base));
            p.circle_filled(t.pos, t.r, base);
            p.text(t.pos, Align2::CENTER_CENTER, names[t.axis], FontId::proportional(11.0), Color32::BLACK);
        } else {
            p.circle_filled(t.pos, t.r, Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 110));
        }
    }
}