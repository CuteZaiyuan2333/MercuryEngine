struct VertexInput { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32> }
struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) world_normal: vec3<f32> }
@group(0) @binding(0) var<uniform> view_proj: mat4x4<f32>;
@group(0) @binding(1) var<uniform> model: mat4x4<f32>;
@vertex fn vs(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = view_proj * model * vec4<f32>(in.position, 1.0);
    out.world_normal = (model * vec4<f32>(in.normal, 0.0)).xyz;
    return out;
}
fn encode_normal(n: vec3<f32>) -> vec3<f32> { return n * 0.5 + 0.5; }
struct FragmentOutput {
    @location(0) gbuffer0: vec4<f32>,
    @location(1) gbuffer1: vec4<f32>,
    @location(2) gbuffer2: vec4<f32>,
    @location(3) gbuffer3: vec4<f32>,
}
@fragment fn fs(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    out.gbuffer0 = vec4<f32>(0.6, 0.6, 0.6, 1.0);
    out.gbuffer1 = vec4<f32>(encode_normal(normalize(in.world_normal)), 1.0 / 3.0);
    out.gbuffer2 = vec4<f32>(0.5, 0.0, 0.5, 0.0);
    out.gbuffer3 = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    return out;
}
