// LoadEngine scene shader: meshes, wireframe, sky, infinite grid, selection outline.

struct Light {
    pos_kind: vec4<f32>,   // xyz: direction-to-light (kind 0) or position (kind 1); w: kind
    color: vec4<f32>,      // rgb: linear color * intensity
};

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
    ambient: vec4<f32>,
    wire_color: vec4<f32>,
    params: vec4<f32>,     // z: wire depth bias
    counts: vec4<u32>,     // x: light count
    lights: array<Light, 8>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var mask_tex: texture_2d<f32>;

// ---------------------------------------------------------------------
//  Meshes
// ---------------------------------------------------------------------

struct MeshIn {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) m0: vec4<f32>,
    @location(3) m1: vec4<f32>,
    @location(4) m2: vec4<f32>,
    @location(5) m3: vec4<f32>,
    @location(6) color: vec4<f32>,   // rgb: base color (sRGB), w: 1 = double sided
};

struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) color: vec4<f32>,
};

fn world_normal(i: MeshIn) -> vec3<f32> {
    // cofactor matrix: correct normals under non-uniform scale, no inverse needed
    let a = i.m0.xyz;
    let b = i.m1.xyz;
    let c = i.m2.xyz;
    let bc = cross(b, c);
    let det = dot(a, bc);
    let sgn = select(1.0, -1.0, det < 0.0);
    return normalize((bc * i.nrm.x + cross(c, a) * i.nrm.y + cross(a, b) * i.nrm.z) * sgn);
}

@vertex
fn vs_mesh(i: MeshIn) -> MeshOut {
    let model = mat4x4<f32>(i.m0, i.m1, i.m2, i.m3);
    let w = model * vec4<f32>(i.pos, 1.0);
    var o: MeshOut;
    o.clip = g.view_proj * w;
    o.world = w.xyz;
    o.nrm = world_normal(i);
    o.color = i.color;
    return o;
}

@fragment
fn fs_mesh(i: MeshOut) -> @location(0) vec4<f32> {
    let v = normalize(g.eye.xyz - i.world);
    var n = normalize(i.nrm);
    if (i.color.w > 0.5 && dot(n, v) < 0.0) {
        n = -n;
    }
    let base = pow(i.color.rgb, vec3<f32>(2.2));
    let hemi = n.y * 0.5 + 0.5;

    var diffuse = vec3<f32>(0.0);
    var spec = vec3<f32>(0.0);
    for (var k = 0u; k < g.counts.x; k = k + 1u) {
        let L = g.lights[k];
        var l = vec3<f32>(0.0, 1.0, 0.0);
        var att = 1.0;
        if (L.pos_kind.w < 0.5) {
            l = normalize(L.pos_kind.xyz);
        } else {
            let d = L.pos_kind.xyz - i.world;
            let d2 = max(dot(d, d), 0.0001);
            l = d * inverseSqrt(d2);
            att = 10.0 / (10.0 + d2);
        }
        let ndl = max(dot(n, l), 0.0);
        diffuse = diffuse + L.color.rgb * (ndl * att);
        let h = normalize(l + v);
        let s = pow(max(dot(n, h), 0.0), 40.0) * 0.2 * att;
        spec = spec + L.color.rgb * (s * step(0.0001, ndl));
    }

    let ambient = g.ambient.rgb * (0.6 + 0.4 * hemi);
    let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * 0.05;
    let col = base * (ambient + diffuse) + spec + ambient * rim;
    return vec4<f32>(col, 1.0);
}

// ---------------------------------------------------------------------
//  Wireframe (depth-biased so it sits on top of the surface)
// ---------------------------------------------------------------------

@vertex
fn vs_wire(i: MeshIn) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(i.m0, i.m1, i.m2, i.m3);
    var clip = g.view_proj * (model * vec4<f32>(i.pos, 1.0));
    clip.z = clip.z * (1.0 + g.params.z);
    return clip;
}

@fragment
fn fs_wire() -> @location(0) vec4<f32> {
    return g.wire_color;
}

// ---------------------------------------------------------------------
//  Selection mask
// ---------------------------------------------------------------------

@vertex
fn vs_mask(i: MeshIn) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(i.m0, i.m1, i.m2, i.m3);
    return g.view_proj * (model * vec4<f32>(i.pos, 1.0));
}

@fragment
fn fs_mask() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0);
}

// ---------------------------------------------------------------------
//  Full-screen passes: sky, grid, outline
// ---------------------------------------------------------------------

struct FsIn {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) ndc: vec2<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> FsIn {
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let xy = p[vi];
    var o: FsIn;
    o.pos = vec4<f32>(xy, 0.0, 1.0);
    o.uv = xy * 0.5 + vec2<f32>(0.5);
    o.ndc = xy;
    return o;
}

@fragment
fn fs_sky(in: FsIn) -> @location(0) vec4<f32> {
    return mix(g.sky_bottom, g.sky_top, clamp(in.uv.y, 0.0, 1.0));
}

fn grid_line(coord: vec2<f32>, dd: vec2<f32>, scale: f32) -> f32 {
    let c = coord / scale;
    let d = max(dd / scale, vec2<f32>(0.000001));
    let gr = abs(fract(c - vec2<f32>(0.5)) - vec2<f32>(0.5)) / d;
    return 1.0 - min(min(gr.x, gr.y), 1.0);
}

struct GridOut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_grid(in: FsIn) -> GridOut {
    // ray from the eye through this pixel, intersected with the plane y = 0
    let nh = g.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let near_p = nh.xyz / nh.w;
    let dir = normalize(near_p - g.eye.xyz);
    let t = -g.eye.y / dir.y;
    let hit = g.eye.xyz + dir * t;
    let coord = hit.xz;
    let dd = fwidth(coord);

    let clip = g.view_proj * vec4<f32>(hit, 1.0);
    let depth = clip.z / clip.w;

    let aa = max(dd.x, dd.y);
    let minor = grid_line(coord, dd, 1.0) * (1.0 - smoothstep(0.06, 0.4, aa));
    let major = grid_line(coord, dd, 10.0) * (1.0 - smoothstep(0.6, 3.0, aa));
    let ax = 1.0 - min(abs(coord.y) / max(dd.y, 0.000001), 1.0);
    let az = 1.0 - min(abs(coord.x) / max(dd.x, 0.000001), 1.0);

    var rgb = vec3<f32>(0.8);
    rgb = mix(rgb, vec3<f32>(0.85, 0.07, 0.07), ax);
    rgb = mix(rgb, vec3<f32>(0.07, 0.2, 0.9), az);
    var a = max(minor * 0.10, major * 0.22);
    a = max(a, max(ax, az) * 0.7);
    let fade = 1.0 - clamp(t / 120.0, 0.0, 1.0);
    a = a * fade * fade;

    let ok = t > 0.0 && clip.w > 0.0 && depth > 0.0 && depth <= 1.0;
    var out: GridOut;
    out.color = vec4<f32>(rgb, select(0.0, a, ok));
    // tiny relative bias so grid lines win against a coplanar ground mesh
    out.depth = select(0.0, depth * 1.00001, ok);
    return out;
}

@fragment
fn fs_outline(in: FsIn) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(mask_tex));
    let p = vec2<i32>(in.pos.xy);
    let c = textureLoad(mask_tex, p, 0).r;
    var sum = 0.0;
    var cnt = 0.0;
    for (var y = -2; y <= 2; y = y + 1) {
        for (var x = -2; x <= 2; x = x + 1) {
            if (x * x + y * y <= 5) {
                let q = clamp(p + vec2<i32>(x, y), vec2<i32>(0), dims - vec2<i32>(1));
                sum = sum + textureLoad(mask_tex, q, 0).r;
                cnt = cnt + 1.0;
            }
        }
    }
    let cover = sum / cnt;
    let a = (1.0 - c) * smoothstep(0.0, 0.3, cover);
    return vec4<f32>(1.0, 0.30, 0.02, a);
}