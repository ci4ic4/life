struct Params {
    w: u32,
    h: u32,
    wrap_x: u32,
    wrap_y: u32,
    birth: u32,
    survive: u32,
    _pad0: u32,
    _pad1: u32,
};
@group(0) @binding(0) var src: texture_2d<u32>;
@group(0) @binding(1) var dst: texture_storage_2d<r32uint, write>;
@group(0) @binding(2) var<uniform> p: Params;

// resolve one axis: in-bounds identity; None -> -1 sentinel; Straight/Flip wrap.
fn resolve_axis(v: i32, n: i32, wrap: u32) -> i32 {
    if (v >= 0 && v < n) { return v; }
    if (wrap == 1u) { return -1; }
    return ((v % n) + n) % n;
}
fn crossings(v: i32, n: i32) -> i32 {
    if (v < 0) { return (-v - 1) / n + 1; }
    return v / n;
}
fn sample(x: i32, y: i32) -> u32 {
    let wi = i32(p.w);
    let hi = i32(p.h);
    var rx = resolve_axis(x, wi, p.wrap_x);
    var ry = resolve_axis(y, hi, p.wrap_y);
    if (rx < 0 || ry < 0) { return 0u; } // hard edge -> dead
    // Flip: crossing x-seam mirrors y, crossing y-seam mirrors x
    if (p.wrap_x == 2u && (x < 0 || x >= wi) && (crossings(x, wi) % 2 == 1)) { ry = hi - 1 - ry; }
    if (p.wrap_y == 2u && (y < 0 || y >= hi) && (crossings(y, hi) % 2 == 1)) { rx = wi - 1 - rx; }
    return textureLoad(src, vec2<i32>(rx, ry), 0).r;
}
fn member(mask: u32, n: u32) -> bool { return (mask & (1u << n)) != 0u; }

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.w || gid.y >= p.h) { return; }
    let x = i32(gid.x);
    let y = i32(gid.y);
    var n: u32 = 0u;
    for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
        for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
            if (dx == 0 && dy == 0) { continue; }
            n = n + sample(x + dx, y + dy);
        }
    }
    let alive = textureLoad(src, vec2<i32>(x, y), 0).r == 1u;
    var next: u32 = 0u;
    if (alive) {
        if (member(p.survive, n)) { next = 1u; }
    } else {
        if (member(p.birth, n)) { next = 1u; }
    }
    textureStore(dst, vec2<i32>(x, y), vec4<u32>(next, 0u, 0u, 0u));
}
