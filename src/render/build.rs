use std::collections::HashMap;

use glam::{Mat4, Vec3, Vec4};

use super::{Batch, InstanceRaw, LightData, Link};
use crate::game::{Component, EntityId, GameState, LightKind, Scene};

const MAX_LIGHTS: usize = 8;

fn lin(c: [f32; 3]) -> Vec3 {
    Vec3::new(c[0].powf(2.2), c[1].powf(2.2), c[2].powf(2.2))
}

/// ماتریس جهانی و دیده‌شدن همه‌ی آبجکت‌ها در یک پیمایش (بدون جستجوی خطی)
fn world_and_visible(scene: &Scene) -> (Vec<Mat4>, Vec<bool>) {
    let n = scene.entities.len();
    let index: HashMap<EntityId, usize> =
        scene.entities.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
    let mut world = vec![Mat4::IDENTITY; n];
    let mut vis = vec![true; n];
    let mut done = vec![false; n];
    let mut chain: Vec<usize> = Vec::new();

    for start in 0..n {
        if done[start] {
            continue;
        }
        chain.clear();
        let mut cur = Some(start);
        let mut guard = 0;
        while let Some(c) = cur {
            if done[c] || guard > 64 {
                break;
            }
            chain.push(c);
            cur = scene.entities[c].parent.and_then(|p| index.get(&p).copied());
            guard += 1;
        }
        let (mut m, mut v) = match cur {
            Some(c) if done[c] => (world[c], vis[c]),
            _ => (Mat4::IDENTITY, true),
        };
        for &c in chain.iter().rev() {
            let e = &scene.entities[c];
            m = m * e.local_matrix();
            v = v && e.active;
            world[c] = m;
            vis[c] = v;
            done[c] = true;
        }
    }
    (world, vis)
}

/// چهار صفحه‌ی کناری Frustum (چپ، راست، پایین، بالا)
fn frustum_planes(m: &Mat4) -> [Vec4; 4] {
    let r0 = Vec4::new(m.x_axis.x, m.y_axis.x, m.z_axis.x, m.w_axis.x);
    let r1 = Vec4::new(m.x_axis.y, m.y_axis.y, m.z_axis.y, m.w_axis.y);
    let r3 = Vec4::new(m.x_axis.w, m.y_axis.w, m.z_axis.w, m.w_axis.w);
    let mut p = [r3 + r0, r3 - r0, r3 + r1, r3 - r1];
    for pl in p.iter_mut() {
        let l = pl.truncate().length();
        if l > 0.0 {
            *pl /= l;
        }
    }
    p
}

fn sphere_visible(p: &[Vec4; 4], c: Vec3, r: f32) -> bool {
    p.iter().all(|pl| pl.truncate().dot(c) + pl.w >= -r)
}

/// درخواست رندر این فریم را می‌سازد. بافرها بین فریم‌ها دوباره استفاده می‌شوند.
pub fn fill_request(
    link: &mut Link,
    gs: &GameState,
    view: Mat4,
    fov_deg: f32,
    aspect: f32,
    eye: Vec3,
    px: [u32; 2],
    playing: bool,
) {
    let Link { req, scratch, settings, .. } = link;

    let proj = Mat4::perspective_infinite_reverse_rh(fov_deg.to_radians(), aspect, 0.05);
    let vp = proj * view;
    let planes = frustum_planes(&vp);
    let (world, vis) = world_and_visible(&gs.scene);

    req.px = px;
    req.samples = settings.msaa;
    req.view_proj = vp;
    req.eye = eye;
    req.shading = gs.shading;
    req.grid = gs.show_grid;
    req.lights.clear();
    req.instances.clear();
    req.batches.clear();
    req.selected = None;
    scratch.clear();

    for (i, e) in gs.scene.entities.iter().enumerate() {
        if !vis[i] {
            continue;
        }
        for c in &e.components {
            match c {
                Component::Light { kind, color, intensity } if req.lights.len() < MAX_LIGHTS => {
                    let w = &world[i];
                    match kind {
                        LightKind::Directional => req.lights.push(LightData {
                            kind: *kind,
                            vec: -w.transform_vector3(Vec3::NEG_Z).normalize_or_zero(),
                            color: lin(*color) * (*intensity * 0.85),
                        }),
                        LightKind::Point => req.lights.push(LightData {
                            kind: *kind,
                            vec: w.transform_point3(Vec3::ZERO),
                            color: lin(*color) * (*intensity * 1.4),
                        }),
                    }
                }
                _ => {}
            }
        }

        let Some((prim, color)) = e.mesh() else { continue };
        let mesh = gs.meshes.get(prim);
        let w = world[i];
        let center = w.w_axis.truncate();
        let s = w
            .x_axis
            .truncate()
            .length()
            .max(w.y_axis.truncate().length())
            .max(w.z_axis.truncate().length());
        if !sphere_visible(&planes, center, mesh.radius * s) {
            continue;
        }
        scratch.push((
            prim,
            InstanceRaw {
                model: w.to_cols_array_2d(),
                color: [color[0], color[1], color[2], if mesh.double_sided { 1.0 } else { 0.0 }],
            },
        ));
    }

    if req.lights.is_empty() {
        req.lights.push(LightData {
            kind: LightKind::Directional,
            vec: Vec3::new(0.4, 1.0, 0.3).normalize(),
            color: Vec3::splat(0.85),
        });
    }

    // گروه‌بندی بر اساس مش تا هر مش با یک Draw Call رسم شود
    scratch.sort_unstable_by_key(|(p, _)| *p);
    for (p, inst) in scratch.iter() {
        let idx = req.instances.len() as u32;
        match req.batches.last_mut() {
            Some(b) if b.prim == *p => b.count += 1,
            _ => req.batches.push(Batch { prim: *p, start: idx, count: 1 }),
        }
        req.instances.push(*inst);
    }

    // آبجکت انتخاب‌شده (برای Outline)
    if !playing {
        if let Some(sel) = gs.selected {
            if let Some(i) = gs.scene.entities.iter().position(|e| e.id == sel) {
                if vis[i] {
                    if let Some((prim, color)) = gs.scene.entities[i].mesh() {
                        req.selected = Some((
                            prim,
                            InstanceRaw {
                                model: world[i].to_cols_array_2d(),
                                color: [color[0], color[1], color[2], 0.0],
                            },
                        ));
                    }
                }
            }
        }
    }

    req.valid = true;
}