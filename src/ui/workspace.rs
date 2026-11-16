use crate::state::AppState;

pub fn modeling(ctx: &egui::Context, _app: &mut AppState) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("🧊 Modeling Workspace (Blender Logic Integration)");
        ui.label("3D mesh generation and modeling tools will be placed here...");
    });
}