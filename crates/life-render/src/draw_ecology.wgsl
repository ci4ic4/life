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
    mode: u32, // 0 species, 1 marten energy
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};
@group(0) @binding(0) var<uniform> u: Uni;
// r species (0 empty, 1 red, 2 grey, 3 marten), g marten energy,
// b infection flag, a terrain. Only r and g are read here.
@group(0) @binding(1) var state: texture_2d<f32>;

@vertex
fn vs(in: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = u.view_proj * vec4<f32>(in.pos, 1.0);
    o.uv = in.uv;
    return o;
}

// Palette lifted from life-ecology.html's cellRGB, so the two read as the same
// simulation. Energy is normalised against the browser's default eCap of 15.
//
// These are the browser's byte values over 255, which are *sRGB*: it writes them
// into a Uint8Array texture. The surface here is an sRGB format (see GpuContext),
// so whatever this shader returns is taken as linear and gamma-encoded on write.
// Returning them as-is renders every colour roughly 3.5x too light — the dark
// ground came out mid-brown. Hence to_linear at the point of use.
const DEAD        = vec3<f32>(0.102, 0.071, 0.031);
const RED_RGB     = vec3<f32>(0.878, 0.325, 0.227);
const GREY_RGB    = vec3<f32>(0.604, 0.643, 0.690);
const MART_DIM    = vec3<f32>(0.235, 0.141, 0.063);
const MART_BRIGHT = vec3<f32>(0.588, 0.376, 0.173);
const E_CAP = 15.0;

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn viridis(t: f32) -> vec3<f32> {
    if (t < 0.5) { return mix(vec3<f32>(0.27, 0.0, 0.33), vec3<f32>(0.13, 0.57, 0.55), t * 2.0); }
    return mix(vec3<f32>(0.13, 0.57, 0.55), vec3<f32>(0.99, 0.9, 0.15), t * 2.0 - 1.0);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let dims = textureDimensions(state);
    let px = vec2<i32>(
        i32(in.uv.x * f32(dims.x)) % i32(dims.x),
        i32(in.uv.y * f32(dims.y)) % i32(dims.y),
    );
    let s = textureLoad(state, px, 0);
    let species = s.r;
    let fed = clamp(s.g / E_CAP, 0.0, 1.0);

    if (u.mode == 1u) {
        // energy mode: only the marten carries an energy ledger, so the squirrels
        // stay flat and the ramp reads as "how close is this animal to breeding".
        // viridis is the renderer's own ramp, shared with evolve mode, and is
        // already authored in linear — it does not go through to_linear.
        if (species == 3.0) { return vec4<f32>(viridis(fed), 1.0); }
        if (species == 0.0) { return vec4<f32>(to_linear(DEAD), 1.0); }
        return vec4<f32>(to_linear(DEAD) * 2.2, 1.0);
    }
    if (species == 1.0) { return vec4<f32>(to_linear(RED_RGB), 1.0); }
    if (species == 2.0) { return vec4<f32>(to_linear(GREY_RGB), 1.0); }
    // the marten mixes in sRGB as the browser does, then converts once
    if (species == 3.0) { return vec4<f32>(to_linear(mix(MART_DIM, MART_BRIGHT, fed)), 1.0); }
    return vec4<f32>(to_linear(DEAD), 1.0);
}
