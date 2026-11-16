use std::path::PathBuf;

use egui::{Align, Color32, Key, Modifiers, RichText};
use glam::Vec3;

use super::viewport;
use crate::render::Link;
use crate::game::mesh::Primitive;
use crate::game::{
    BottomTab, Component, EntityId, GameState, LightKind, PlayMode, Shading, Tool,
};
use crate::state::{AppState, Workspace};

// =====================================================================
//  Entry point
// =====================================================================

pub fn draw(ctx: &egui::Context, app: &mut AppState) {
    let scripts: Vec<PathBuf> = app
        .editor
        .files
        .iter()
        .filter(|p| p.extension().map_or(false, |e| e == "les"))
        .cloned()
        .collect();
    let mut open_script: Option<PathBuf> = None;

    let dt = ctx.input(|i| i.stable_dt).min(0.05);
    let down = ctx.input(|i| i.pointer.primary_down());

    let gs = &mut app.game;
    let link = &mut app.link;
    if gs.mode == PlayMode::Playing {
        ctx.request_repaint();
    }
    gs.begin_frame();
    hotkeys(ctx, gs);

    toolbar(ctx, gs, link);
    bottom_panel(ctx, gs, &scripts, &mut open_script);
    hierarchy(ctx, gs);
    inspector(ctx, gs, &scripts, &mut open_script);
    egui::CentralPanel::default()
        .frame(egui::Frame::none())
        .show(ctx, |ui| viewport::show(ui, gs, link));

    gs.step(dt);
    gs.commit(down);

    if let Some(id) = app.game.edit_request.take() {
        app.modeling.load_entity(&mut app.game, id);
        app.active_workspace = Workspace::Modeling;
    }
}

// =====================================================================
//  Hotkeys
// =====================================================================

fn hotkeys(ctx: &egui::Context, gs: &mut GameState) {
    if ctx.wants_keyboard_input() {
        return;
    }
    let rmb = ctx.input(|i| i.pointer.secondary_down());
    let edit = gs.mode == PlayMode::Edit;

    let redo = ctx.input_mut(|i| {
        i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
            | i.consume_key(Modifiers::COMMAND, Key::Y)
    });
    let undo = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z));
    let dup = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::D));
    let play = ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::P));
    let del = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete));
    let focus = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F));
    let edit_mesh = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab));
    if edit_mesh && edit {
        if let Some(s) = gs.selected {
            gs.edit_request = Some(s);
        }
    }

    if edit {
        if redo {
            gs.redo();
        } else if undo {
            gs.undo();
        }
    }
    if dup {
        if let Some(s) = gs.selected {
            gs.duplicate(s);
        }
    }
    if del {
        if let Some(s) = gs.selected {
            gs.delete(s);
        }
    }
    if focus {
        if let Some(s) = gs.selected {
            gs.focus(s);
        }
    }
    if play {
        if gs.mode == PlayMode::Edit { gs.play() } else { gs.stop() }
    }
    if !rmb {
        let (q, w, e, r) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::Q),
                i.consume_key(Modifiers::NONE, Key::W),
                i.consume_key(Modifiers::NONE, Key::E),
                i.consume_key(Modifiers::NONE, Key::R),
            )
        });
        if q { gs.tool = Tool::Select; }
        if w { gs.tool = Tool::Move; }
        if e { gs.tool = Tool::Rotate; }
        if r { gs.tool = Tool::Scale; }
    }
}

// =====================================================================
//  Toolbar
// =====================================================================

fn toolbar(ctx: &egui::Context, gs: &mut GameState, link: &mut Link) {
    egui::TopBottomPanel::top("game_toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            for (t, label, tip) in [
                (Tool::Select, "🖱", "Select (Q)"),
                (Tool::Move, "✥", "Move (W)"),
                (Tool::Rotate, "⟳", "Rotate (E)"),
                (Tool::Scale, "⤢", "Scale (R)"),
            ] {
                if ui
                    .selectable_label(gs.tool == t, RichText::new(label).size(18.0))
                    .on_hover_text(tip)
                    .clicked()
                {
                    gs.tool = t;
                }
            }
            ui.separator();
            ui.checkbox(&mut gs.snap, "🧲 Snap");
            ui.checkbox(&mut gs.show_grid, "▦ Grid");
            egui::ComboBox::from_id_source("shading_mode")
                .selected_text(gs.shading.label())
                .show_ui(ui, |ui| {
                    for s in Shading::ALL {
                        ui.selectable_value(&mut gs.shading, s, s.label());
                    }
                });

            ui.menu_button("⚙ Render", |ui| {
                ui.label("Anti-aliasing");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut link.settings.msaa, 1, "Off");
                    ui.selectable_value(&mut link.settings.msaa, 4, "MSAA 4x");
                });
                ui.add(egui::Slider::new(&mut link.settings.scale, 0.5..=1.0).text("Resolution scale"));
            });

            ui.separator();

            let edit = gs.mode == PlayMode::Edit;
            if ui
                .add_enabled(edit && !gs.undo.is_empty(), egui::Button::new("↶"))
                .on_hover_text("Undo (Ctrl+Z)")
                .clicked()
            {
                gs.undo();
            }
            if ui
                .add_enabled(edit && !gs.redo.is_empty(), egui::Button::new("↷"))
                .on_hover_text("Redo (Ctrl+Y)")
                .clicked()
            {
                gs.redo();
            }
            ui.separator();

            match gs.mode {
                PlayMode::Edit => {
                    let b = egui::Button::new(RichText::new("▶ Play").color(Color32::WHITE))
                        .fill(Color32::from_rgb(46, 125, 50));
                    if ui.add(b).on_hover_text("Ctrl+P").clicked() {
                        gs.play();
                    }
                }
                PlayMode::Playing => {
                    if ui.button("⏸ Pause").clicked() {
                        gs.pause();
                    }
                    let b = egui::Button::new(RichText::new("⏹ Stop").color(Color32::WHITE))
                        .fill(Color32::from_rgb(183, 28, 28));
                    if ui.add(b).clicked() {
                        gs.stop();
                    }
                }
                PlayMode::Paused => {
                    if ui.button("▶ Resume").clicked() {
                        gs.resume();
                    }
                    let b = egui::Button::new(RichText::new("⏹ Stop").color(Color32::WHITE))
                        .fill(Color32::from_rgb(183, 28, 28));
                    if ui.add(b).clicked() {
                        gs.stop();
                    }
                }
            }

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.weak(format!("{} tris · {} draws", link.tris, link.draw_calls));
                if gs.mode != PlayMode::Edit {
                    ui.weak(format!("{:.0} FPS", gs.fps));
                }
            });
        });
    });
}

// =====================================================================
//  Create menu (shared)
// =====================================================================

fn create_menu(ui: &mut egui::Ui, gs: &mut GameState, parent: Option<EntityId>) {
    if ui.button("◇ Empty").clicked() {
        gs.create("Empty", vec![], parent);
        ui.close_menu();
    }
    ui.menu_button("📦 3D Object", |ui| {
        for p in Primitive::ALL {
            if ui.button(format!("{} {}", p.icon(), p.name())).clicked() {
                gs.create_primitive(p, parent);
                ui.close_menu();
            }
        }
    });
    ui.menu_button("💡 Light", |ui| {
        if ui.button("Directional Light").clicked() {
            let id = gs.create(
                "Directional Light",
                vec![Component::Light { kind: LightKind::Directional, color: [1.0, 0.96, 0.88], intensity: 1.0 }],
                parent,
            );
            if let Some(e) = gs.scene.get_mut(id) {
                e.rot = Vec3::new(-50.0, -30.0, 0.0);
            }
            ui.close_menu();
        }
        if ui.button("Point Light").clicked() {
            let id = gs.create(
                "Point Light",
                vec![Component::Light { kind: LightKind::Point, color: [1.0, 1.0, 1.0], intensity: 1.0 }],
                parent,
            );
            if let Some(e) = gs.scene.get_mut(id) {
                e.pos.y += 2.5;
            }
            ui.close_menu();
        }
    });
    if ui.button("📷 Camera").clicked() {
        gs.create("Camera", vec![Component::Camera { fov: 60.0 }], parent);
        ui.close_menu();
    }
}

// =====================================================================
//  Hierarchy
// =====================================================================

enum Act {
    Select(EntityId),
    Expand(EntityId),
    Active(EntityId),
    Dup(EntityId),
    Del(EntityId),
    Focus(EntityId),
    Model(EntityId),
    Parent(EntityId, Option<EntityId>),
}

struct Row {
    id: EntityId,
    depth: usize,
    name: String,
    icon: &'static str,
    has_children: bool,
    has_parent: bool,
    expanded: bool,
    active: bool,
    selected: bool,
}

fn hierarchy(ctx: &egui::Context, gs: &mut GameState) {
    let mut acts: Vec<Act> = Vec::new();

    egui::SidePanel::left("game_hierarchy")
        .resizable(true)
        .default_width(230.0)
        .min_width(170.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("🌳 Hierarchy");
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.menu_button("＋ Create", |ui| create_menu(ui, gs, None));
                });
            });
            ui.add(
                egui::TextEdit::singleline(&mut gs.filter)
                    .hint_text("🔍 Search...")
                    .desired_width(f32::INFINITY),
            );
            ui.separator();

            let filtering = !gs.filter.trim().is_empty();
            let order: Vec<(EntityId, usize)> = if filtering {
                let f = gs.filter.to_lowercase();
                gs.scene
                    .entities
                    .iter()
                    .filter(|e| e.name.to_lowercase().contains(&f))
                    .map(|e| (e.id, 0))
                    .collect()
            } else {
                gs.scene.dfs_order()
            };
            let all: Vec<(EntityId, String)> =
                gs.scene.entities.iter().map(|e| (e.id, e.name.clone())).collect();

            let mut rows: Vec<Row> = Vec::new();
            let mut skip: Option<usize> = None;
            for (id, depth) in order {
                if !filtering {
                    if let Some(d) = skip {
                        if depth > d {
                            continue;
                        }
                        skip = None;
                    }
                }
                let Some(e) = gs.scene.get(id) else { continue };
                let has_children = !gs.scene.children(id).is_empty();
                let expanded = !gs.collapsed.contains(&id);
                if !filtering && has_children && !expanded {
                    skip = Some(depth);
                }
                rows.push(Row {
                    id,
                    depth,
                    name: e.name.clone(),
                    icon: e.icon(),
                    has_children,
                    has_parent: e.parent.is_some(),
                    expanded,
                    active: e.active,
                    selected: gs.selected == Some(id),
                });
            }

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for row in &rows {
                    ui.horizontal(|ui| {
                        ui.add_space(row.depth as f32 * 14.0);
                        if row.has_children && !filtering {
                            if ui.small_button(if row.expanded { "▾" } else { "▸" }).clicked() {
                                acts.push(Act::Expand(row.id));
                            }
                        } else {
                            ui.add_space(22.0);
                        }
                        if ui
                            .small_button(if row.active { "👁" } else { "—" })
                            .on_hover_text("Toggle active")
                            .clicked()
                        {
                            acts.push(Act::Active(row.id));
                        }

                        let mut text = RichText::new(format!("{} {}", row.icon, row.name));
                        if !row.active {
                            text = text.weak();
                        }
                        let r = ui.selectable_label(row.selected, text);
                        if r.clicked() {
                            acts.push(Act::Select(row.id));
                        }
                        if r.double_clicked() {
                            acts.push(Act::Focus(row.id));
                        }
                        let id = row.id;
                        r.context_menu(|ui| {
                            if ui.button("🎯 Focus").clicked() {
                                acts.push(Act::Focus(id));
                                ui.close_menu();
                            }
                            if ui.button("🧊 Edit in Modeling  (Tab)").clicked() {
                                acts.push(Act::Model(id));
                                ui.close_menu();
                            }
                            if ui.button("⧉ Duplicate  (Ctrl+D)").clicked() {
                                acts.push(Act::Dup(id));
                                ui.close_menu();
                            }
                            if ui.button("🗑 Delete  (Del)").clicked() {
                                acts.push(Act::Del(id));
                                ui.close_menu();
                            }
                            ui.separator();
                            ui.menu_button("➕ Add child", |ui| create_menu(ui, gs, Some(id)));
                            if row.has_parent && ui.button("⇧ Unparent").clicked() {
                                acts.push(Act::Parent(id, None));
                                ui.close_menu();
                            }
                            ui.menu_button("👪 Set parent", |ui| {
                                for (pid, pname) in &all {
                                    if *pid == id || gs.scene.is_descendant(*pid, id) {
                                        continue;
                                    }
                                    if ui.button(pname).clicked() {
                                        acts.push(Act::Parent(id, Some(*pid)));
                                        ui.close_menu();
                                    }
                                }
                            });
                        });
                    });
                }
                if rows.is_empty() {
                    ui.weak("Empty scene. Use ＋ Create");
                }
            });
        });

    for a in acts {
        match a {
            Act::Select(id) => gs.selected = Some(id),
            Act::Expand(id) => {
                if !gs.collapsed.remove(&id) {
                    gs.collapsed.insert(id);
                }
            }
            Act::Active(id) => {
                if let Some(e) = gs.scene.get_mut(id) {
                    e.active = !e.active;
                }
            }
            Act::Dup(id) => gs.duplicate(id),
            Act::Del(id) => gs.delete(id),
            Act::Focus(id) => {
                gs.selected = Some(id);
                gs.focus(id);
            }
            Act::Model(id) => gs.edit_request = Some(id),
            Act::Parent(id, p) => gs.scene.set_parent(id, p),
        }
    }
}

// =====================================================================
//  Inspector
// =====================================================================

fn vec3_row(ui: &mut egui::Ui, label: &str, v: &mut Vec3, speed: f64) {
    ui.horizontal(|ui| {
        ui.add_sized([62.0, 18.0], egui::Label::new(label));
        ui.spacing_mut().item_spacing.x = 3.0;
        ui.colored_label(Color32::from_rgb(235, 75, 75), "X");
        ui.add(egui::DragValue::new(&mut v.x).speed(speed).max_decimals(2));
        ui.colored_label(Color32::from_rgb(110, 205, 85), "Y");
        ui.add(egui::DragValue::new(&mut v.y).speed(speed).max_decimals(2));
        ui.colored_label(Color32::from_rgb(75, 125, 245), "Z");
        ui.add(egui::DragValue::new(&mut v.z).speed(speed).max_decimals(2));
    });
}

fn inspector(
    ctx: &egui::Context,
    gs: &mut GameState,
    scripts: &[PathBuf],
    open: &mut Option<PathBuf>,
) {
    egui::SidePanel::right("game_inspector")
        .resizable(true)
        .default_width(300.0)
        .min_width(250.0)
        .show(ctx, |ui| {
            ui.strong("🔍 Inspector");
            ui.separator();
            if gs.mode != PlayMode::Edit {
                ui.colored_label(
                    Color32::from_rgb(255, 190, 80),
                    "⚠ Play mode: changes are reverted on Stop",
                );
            }
            let Some(sel) = gs.selected else {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.weak("Nothing selected");
                    ui.weak("Click an object in the viewport or hierarchy");
                });
                return;
            };
            let Some(e) = gs.scene.get_mut(sel) else { return };

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut e.active, "");
                    ui.add(egui::TextEdit::singleline(&mut e.name).desired_width(f32::INFINITY));
                });
                ui.add_space(4.0);

                egui::CollapsingHeader::new(RichText::new("📐 Transform").strong())
                    .default_open(true)
                    .show(ui, |ui| {
                        vec3_row(ui, "Position", &mut e.pos, 0.05);
                        vec3_row(ui, "Rotation", &mut e.rot, 0.5);
                        vec3_row(ui, "Scale", &mut e.scale, 0.02);
                        if ui.small_button("↺ Reset transform").clicked() {
                            e.pos = Vec3::ZERO;
                            e.rot = Vec3::ZERO;
                            e.scale = Vec3::ONE;
                        }
                    });

                let eid = e.id;
                let mut remove: Option<usize> = None;
                for (idx, comp) in e.components.iter_mut().enumerate() {
                    let title = format!("{} {}", comp.icon(), comp.name());
                    egui::CollapsingHeader::new(RichText::new(title).strong())
                        .id_source((eid, idx))
                        .default_open(true)
                        .show(ui, |ui| {
                            match comp {
                                Component::Mesh { prim, color } => {
                                    ui.horizontal(|ui| {
                                        ui.label("Shape");
                                        egui::ComboBox::from_id_source((eid, idx, "prim"))
                                            .selected_text(prim.name())
                                            .show_ui(ui, |ui| {
                                                for p in Primitive::ALL {
                                                    ui.selectable_value(&mut *prim, p, p.name());
                                                }
                                            });
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Color");
                                        ui.color_edit_button_rgb(color);
                                    });
                                }
                                Component::Light { kind, color, intensity } => {
                                    ui.horizontal(|ui| {
                                        ui.label("Type");
                                        egui::ComboBox::from_id_source((eid, idx, "light"))
                                            .selected_text(kind.name())
                                            .show_ui(ui, |ui| {
                                                for k in [LightKind::Directional, LightKind::Point] {
                                                    ui.selectable_value(&mut *kind, k, k.name());
                                                }
                                            });
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("Color");
                                        ui.color_edit_button_rgb(color);
                                    });
                                    ui.add(egui::Slider::new(intensity, 0.0..=3.0).text("Intensity"));
                                }
                                Component::Camera { fov } => {
                                    ui.add(egui::Slider::new(fov, 20.0..=120.0).suffix("°").text("FOV"));
                                    ui.weak("The first active camera is used in Play mode.");
                                }
                                Component::Script { path } => {
                                    let current = if path.is_empty() {
                                        "(none)".to_string()
                                    } else {
                                        path.rsplit(['/', '\\']).next().unwrap_or("").to_string()
                                    };
                                    egui::ComboBox::from_id_source((eid, idx, "script"))
                                        .selected_text(current)
                                        .show_ui(ui, |ui| {
                                            for s in scripts {
                                                let sp = s.to_string_lossy().to_string();
                                                let nm = s
                                                    .file_name()
                                                    .map(|n| n.to_string_lossy().to_string())
                                                    .unwrap_or_default();
                                                ui.selectable_value(&mut *path, sp, nm);
                                            }
                                        });
                                    if !path.is_empty() && ui.button("✏ Open in Code Editor").clicked() {
                                        *open = Some(PathBuf::from(path.as_str()));
                                    }
                                }
                                Component::Spin { speed } => {
                                    ui.horizontal(|ui| {
                                        ui.label("Speed");
                                        ui.add(egui::DragValue::new(speed).speed(1.0).suffix(" °/s"));
                                    });
                                }
                                Component::Rigidbody { gravity, bounce } => {
                                    ui.checkbox(gravity, "Use gravity");
                                    ui.add(egui::Slider::new(bounce, 0.0..=1.0).text("Bounce"));
                                    ui.weak("Falls onto the ground (y = 0) in Play mode.");
                                }
                            }
                            if ui.small_button("🗑 Remove component").clicked() {
                                remove = Some(idx);
                            }
                        });
                }
                if let Some(i) = remove {
                    e.components.remove(i);
                }

                ui.add_space(8.0);
                let mut new_comp: Option<Component> = None;
                ui.menu_button("➕ Add Component", |ui| {
                    for c in Component::templates() {
                        if ui.button(format!("{} {}", c.icon(), c.name())).clicked() {
                            new_comp = Some(c);
                            ui.close_menu();
                        }
                    }
                });
                if let Some(c) = new_comp {
                    e.components.push(c);
                }
            });
        });
}

// =====================================================================
//  Bottom panel (Project / Console)
// =====================================================================

fn bottom_panel(
    ctx: &egui::Context,
    gs: &mut GameState,
    scripts: &[PathBuf],
    open: &mut Option<PathBuf>,
) {
    let mut attach: Option<String> = None;

    egui::TopBottomPanel::bottom("game_bottom")
        .resizable(true)
        .default_height(170.0)
        .min_height(90.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut gs.bottom_tab, BottomTab::Project, "📁 Project");
                ui.selectable_value(&mut gs.bottom_tab, BottomTab::Console, "📟 Console");
                if gs.bottom_tab == BottomTab::Console {
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("🗑 Clear").clicked() {
                            gs.log.clear();
                        }
                    });
                }
            });
            ui.separator();

            match gs.bottom_tab {
                BottomTab::Project => {
                    ui.label(RichText::new("Primitives (click to add to the scene)").weak());
                    ui.horizontal_wrapped(|ui| {
                        for p in Primitive::ALL {
                            if ui.button(format!("{} {}", p.icon(), p.name())).clicked() {
                                gs.create_primitive(p, None);
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("Scripts  (double-click to open, right-click to attach)").weak());
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for s in scripts {
                                let name = s
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                let r = ui.selectable_label(false, format!("📜 {name}"));
                                if r.double_clicked() {
                                    *open = Some(s.clone());
                                }
                                r.context_menu(|ui| {
                                    if ui.button("✏ Open in Code Editor").clicked() {
                                        *open = Some(s.clone());
                                        ui.close_menu();
                                    }
                                    if ui.button("📎 Attach to selected object").clicked() {
                                        attach = Some(s.to_string_lossy().to_string());
                                        ui.close_menu();
                                    }
                                });
                            }
                        });
                        if scripts.is_empty() {
                            ui.weak("No scripts found in scripts/");
                        }
                    });
                }
                BottomTab::Console => {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for l in &gs.log {
                                ui.label(RichText::new(l).monospace());
                            }
                        });
                }
            }
        });

    if let Some(p) = attach {
        if let Some(sel) = gs.selected {
            if let Some(e) = gs.scene.get_mut(sel) {
                e.components.push(Component::Script { path: p });
            }
        }
    }
}