struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
struct Uni {
    view_proj: mat4x4<f32>,
    mode: u32, // 0 clan, 1 tau (viridis), 2 theta (magma), 3 env
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};
@group(0) @binding(0) var<uniform> u: Uni;
@group(0) @binding(1) var state: texture_2d<f32>;
@group(0) @binding(2) var env_tex: texture_2d<f32>;

@vertex
fn vs(in: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = u.view_proj * vec4<f32>(in.pos, 1.0);
    o.uv = in.uv;
    return o;
}

// 3-stop colour ramps (approximations of the browser's viridis/magma)
fn ramp3(a: vec3<f32>, b: vec3<f32>, c: vec3<f32>, t: f32) -> vec3<f32> {
    if (t < 0.5) { return mix(a, b, t * 2.0); }
    return mix(b, c, t * 2.0 - 1.0);
}
fn viridis(t: f32) -> vec3<f32> {
    return ramp3(vec3<f32>(0.27, 0.0, 0.33), vec3<f32>(0.13, 0.57, 0.55), vec3<f32>(0.99, 0.9, 0.15), t);
}
fn magma(t: f32) -> vec3<f32> {
    return ramp3(vec3<f32>(0.0, 0.0, 0.02), vec3<f32>(0.72, 0.21, 0.47), vec3<f32>(0.99, 0.99, 0.75), t);
}
// diverging terrain ramp: hostile red-brown -> neutral dark -> fertile green
fn terrain(e: f32) -> vec3<f32> {
    if (e < 0.0) { return mix(vec3<f32>(0.08, 0.08, 0.14), vec3<f32>(0.55, 0.18, 0.1), -e); }
    return mix(vec3<f32>(0.08, 0.08, 0.14), vec3<f32>(0.15, 0.55, 0.25), e);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let dims = textureDimensions(state);
    let px = vec2<i32>(
        i32(in.uv.x * f32(dims.x)) % i32(dims.x),
        i32(in.uv.y * f32(dims.y)) % i32(dims.y),
    );
    let s = textureLoad(state, px, 0);
    let e = textureLoad(env_tex, px, 0).r;
    let clan = s.r;

    if (u.mode == 3u) {
        // env mode: terrain everywhere, live cells as bright dots
        if (clan != 0.0) { return vec4<f32>(0.95, 0.95, 0.95, 1.0); }
        return vec4<f32>(terrain(e), 1.0);
    }
    if (clan == 0.0) {
        // dead: faint terrain tint so basins/ridges stay legible in all modes
        return vec4<f32>(terrain(e) * 0.45, 1.0);
    }
    if (u.mode == 1u) { return vec4<f32>(viridis(s.g), 1.0); }
    if (u.mode == 2u) { return vec4<f32>(magma(s.b), 1.0); }
    // clan mode: warm vs cool
    if (clan == 1.0) { return vec4<f32>(1.0, 0.55, 0.25, 1.0); }
    return vec4<f32>(0.3, 0.6, 1.0, 1.0);
}
