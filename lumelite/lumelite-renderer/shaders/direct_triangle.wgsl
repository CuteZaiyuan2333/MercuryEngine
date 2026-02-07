// Direct triangle: stride 32 (position, normal, uv). Debug path.
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) _normal: vec3<f32>,
    @location(2) _uv: vec2<f32>,
}
@group(0) @binding(0) var<uniform> view_proj: mat4x4<f32>;
@group(0) @binding(1) var<uniform> model: mat4x4<f32>;
@vertex fn vs(in: VertexInput) -> @builtin(position) vec4<f32> {
    return view_proj * model * vec4<f32>(in.position, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> {
    return vec4<f32>(0.6, 0.6, 0.6, 1.0);
}
