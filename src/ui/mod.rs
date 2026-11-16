pub mod editor;
pub mod game_engine;
pub mod modeling;
pub mod viewport;

use crate::state::{AppState, Workspace};

const TABS: &[(Workspace, &str)] = &[
    (Workspace::GameEngine, "🎮 Game Engine"),
    (Workspace::Modeling, "🧊 Modeling"),
    (Workspace::CodeEditor, "💻 Code Editor"),
];

pub fn draw(ctx: &egui::Context, app: &mut AppState) {
    top_bar(ctx, app);

    match app.active_workspace {
        Workspace::GameEngine => game_engine::draw(ctx, app),
        Workspace::Modeling => modeling::draw(ctx, app),
        Workspace::CodeEditor => editor::code_editor(ctx, app),
    }
}

fn top_bar(ctx: &egui::Context, app: &mut AppState) {
    egui::TopBottomPanel::top("workspace_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.heading("LoadEngine");
            ui.separator();
            for (ws, label) in TABS {
                if ui
                    .selectable_label(app.active_workspace == *ws, *label)
                    .clicked()
                {
                    app.active_workspace = *ws;
                }
            }
        });
    });
}