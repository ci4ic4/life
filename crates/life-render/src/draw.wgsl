struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@group(0) @binding(0) var<uniform> view_proj: mat4x4<f32>;
@group(0) @binding(1) var state: texture_2d<u32>;

@vertex
fn vs(in: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = view_proj * vec4<f32>(in.pos, 1.0);
    o.uv = in.uv;
    return o;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let dims = textureDimensions(state);
    let px = vec2<i32>(
        i32(in.uv.x * f32(dims.x)) % i32(dims.x),
        i32(in.uv.y * f32(dims.y)) % i32(dims.y),
    );
    let alive = textureLoad(state, px, 0).r;
    if (alive == 1u) { return vec4<f32>(0.85, 0.9, 1.0, 1.0); }
    return vec4<f32>(0.05, 0.06, 0.12, 1.0);
}
