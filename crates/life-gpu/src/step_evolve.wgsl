// Evolve step: state texel R = clan (0/1/2 as f32), G = tau, B = theta.
// env is a separate r32float texture. Mirrors life-core::step_evolve exactly
// (same kernel iteration order, same floor(s+0.5) rounding) so the boot
// self-test can assert bit parity at mut_sigma = 0.

struct Params {
    w: u32,
    h: u32,
    wrap_x: u32,
    wrap_y: u32,
    env_weight: f32,
    mut_sigma: f32,
    generation: u32,
    seed: u32,
    rule_mode: u32,      // 0 = digit-set, 1 = ranged
    birth_mask: u32,
    survive_mask: u32,
    kernel_len: u32,
    b_lo: f32,
    b_hi: f32,
    s_lo: f32,
    s_hi: f32,
    kernel: array<vec4<f32>, 25>, // (dc, dr, weight, 0)
};
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var env_tex: texture_2d<f32>;
@group(0) @binding(3) var<uniform> p: Params;

fn pcg(v: u32) -> u32 {
    var s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}
fn rand01(salt: u32) -> f32 {
    return f32(pcg(salt ^ pcg(p.generation * 2654435761u + p.seed))) / 4294967296.0;
}
// Box–Muller, same form as CPU gauss(); draws two uniforms per call.
fn gauss(sigma: f32, salt: u32) -> f32 {
    if (sigma <= 0.0) { return 0.0; }
    let u = 1.0 - rand01(salt);
    let v = rand01(salt ^ 0x9E3779B9u);
    return sigma * sqrt(-2.0 * log(u)) * cos(6.283185307179586 * v);
}

fn resolve_axis(v: i32, n: i32, wrap: u32) -> i32 {
    if (v >= 0 && v < n) { return v; }
    if (wrap == 1u) { return -1; }
    return ((v % n) + n) % n;
}
fn crossings(v: i32, n: i32) -> i32 {
    if (v < 0) { return (-v - 1) / n + 1; }
    return v / n;
}
// resolve to a texel coord, or (-1,-1) off a hard edge
fn resolve(x: i32, y: i32) -> vec2<i32> {
    let wi = i32(p.w);
    let hi = i32(p.h);
    var rx = resolve_axis(x, wi, p.wrap_x);
    var ry = resolve_axis(y, hi, p.wrap_y);
    if (rx < 0 || ry < 0) { return vec2<i32>(-1, -1); }
    if (p.wrap_x == 2u && (x < 0 || x >= wi) && (crossings(x, wi) % 2 == 1)) { ry = hi - 1 - ry; }
    if (p.wrap_y == 2u && (y < 0 || y >= hi) && (crossings(y, hi) % 2 == 1)) { rx = wi - 1 - rx; }
    return vec2<i32>(rx, ry);
}

// JS Math.round: floor(x + 0.5)
fn js_round(x: f32) -> f32 { return floor(x + 0.5); }

fn birth_ok(n: f32) -> bool {
    if (p.rule_mode == 1u) { return n >= p.b_lo && n <= p.b_hi; }
    let k = js_round(n);
    if (k < 0.0 || k > 8.0) { return false; }
    return (p.birth_mask & (1u << u32(k))) != 0u;
}
fn survive_ok(s: f32) -> bool {
    if (p.rule_mode == 1u) { return s >= p.s_lo && s <= p.s_hi; }
    let k = js_round(s);
    if (k < 0.0 || k > 8.0) { return false; }
    return (p.survive_mask & (1u << u32(k))) != 0u;
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.w || gid.y >= p.h) { return; }
    let x = i32(gid.x);
    let y = i32(gid.y);

    var a0 = 0.0; var a1 = 0.0;
    var t_sum0 = 0.0; var t_sum1 = 0.0;
    var th_sum0 = 0.0; var th_sum1 = 0.0;
    for (var i = 0u; i < p.kernel_len; i = i + 1u) {
        let kc = p.kernel[i];
        let pos = resolve(x + i32(kc.x), y + i32(kc.y));
        if (pos.x < 0) { continue; }
        let texel = textureLoad(src, pos, 0);
        let clan = texel.r;
        if (clan == 1.0) {
            a0 = a0 + kc.z; t_sum0 = t_sum0 + kc.z * texel.g; th_sum0 = th_sum0 + kc.z * texel.b;
        } else if (clan == 2.0) {
            a1 = a1 + kc.z; t_sum1 = t_sum1 + kc.z * texel.g; th_sum1 = th_sum1 + kc.z * texel.b;
        }
    }

    let here = textureLoad(src, vec2<i32>(x, y), 0);
    let e_full = p.env_weight * textureLoad(env_tex, vec2<i32>(x, y), 0).r;
    var out = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    if (here.r != 0.0) {
        var allies = a1; var foes = a0;
        if (here.r == 1.0) { allies = a0; foes = a1; }
        let s = allies - (1.0 - here.g) * foes + e_full * (1.0 - here.b);
        if (survive_ok(s)) { out = vec4<f32>(here.r, here.g, here.b, 0.0); }
    } else {
        var eb0 = e_full; var eb1 = e_full;
        if (a0 > 0.0) { eb0 = e_full * (1.0 - th_sum0 / a0); }
        if (a1 > 0.0) { eb1 = e_full * (1.0 - th_sum1 / a1); }
        let b0 = birth_ok(a0 - a1 + eb0) && a0 > 0.0;
        let b1 = birth_ok(a1 - a0 + eb1) && a1 > 0.0;
        if (b0 != b1) {
            var clan: f32; var mt: f32; var mth: f32;
            if (b0) {
                clan = 1.0; mt = t_sum0 / a0; mth = th_sum0 / a0;
            } else {
                clan = 2.0; mt = t_sum1 / a1; mth = th_sum1 / a1;
            }
            let cell = gid.y * p.w + gid.x;
            let tau = clamp(mt + gauss(p.mut_sigma, cell * 2u), 0.0, 1.0);
            let theta = clamp(mth + gauss(p.mut_sigma, cell * 2u + 1u), 0.0, 1.0);
            out = vec4<f32>(clan, tau, theta, 0.0);
        }
    }
    textureStore(dst, vec2<i32>(x, y), out);
}
