// Shadow map pass: render depth from light view (orthographic).

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) depth: f32,
}

@group(0) @binding(0) var<uniform> light_view_proj: mat4x4<f32>;
@group(0) @binding(1) var<uniform> model: mat4x4<f32>;

@vertex fn vs(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let clip = light_view_proj * model * vec4<f32>(in.position, 1.0);
    out.clip_position = clip;
    out.depth = clip.z / clip.w;
    return out;
}

@fragment fn fs(in: VertexOutput) -> @builtin(frag_depth) f32 {
    return in.depth;
}
