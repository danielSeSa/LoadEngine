//! GPU scene rendering. The UI only fills a `SceneRequest`;
//! `gpu::SceneRenderer` turns it into pixels.

pub mod build;
pub mod gpu;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

use crate::game::mesh::Primitive;
use crate::game::{LightKind, Shading};

pub use build::fill_request;
pub use gpu::SceneRenderer;

/// داده‌ی هر نمونه (instance) روی GPU: ۸۰ بایت
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct InstanceRaw {
    /// ماتریس مدل (ستونی)
    pub model: [[f32; 4]; 4],
    /// rgb = رنگ پایه (sRGB)، w = 1.0 برای مش دوطرفه
    pub color: [f32; 4],
}

#[derive(Clone, Copy)]
pub struct LightData {
    pub kind: LightKind,
    /// Directional: جهت «به سمت نور»؛ Point: موقعیت
    pub vec: Vec3,
    /// رنگ خطی × شدت
    pub color: Vec3,
}

/// بازه‌ی نمونه‌های یک مش در بافر Instance
#[derive(Clone, Copy)]
pub struct Batch {
    pub prim: Primitive,
    pub start: u32,
    pub count: u32,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Settings {
    /// 1 = خاموش، 4 = MSAA 4x
    pub msaa: u32,
    /// مقیاس رزولوشن رندر (0.5 تا 1.0)
    pub scale: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { msaa: 4, scale: 1.0 }
    }
}

pub struct SceneRequest {
    pub valid: bool,
    pub px: [u32; 2],
    pub samples: u32,
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub shading: Shading,
    pub grid: bool,
    pub lights: Vec<LightData>,
    pub instances: Vec<InstanceRaw>,
    pub batches: Vec<Batch>,
    pub selected: Option<(Primitive, InstanceRaw)>,
}

impl Default for SceneRequest {
    fn default() -> Self {
        Self {
            valid: false,
            px: [1, 1],
            samples: 4,
            view_proj: Mat4::IDENTITY,
            eye: Vec3::ZERO,
            shading: Shading::Solid,
            grid: true,
            lights: Vec::new(),
            instances: Vec::new(),
            batches: Vec::new(),
            selected: None,
        }
    }
}

/// پل بین UI و رندرر (داخل AppState)
pub struct Link {
    pub tex: egui::TextureId,
    pub settings: Settings,
    pub req: SceneRequest,
    pub draw_calls: u32,
    pub tris: u32,
    scratch: Vec<(Primitive, InstanceRaw)>,
}

impl Link {
    pub fn new() -> Self {
        Self {
            tex: egui::TextureId::default(),
            settings: Settings::default(),
            req: SceneRequest::default(),
            draw_calls: 0,
            tris: 0,
            scratch: Vec::new(),
        }
    }
}