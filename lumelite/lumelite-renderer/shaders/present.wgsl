struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    out.uv = vec2<f32>(x, y);
    out.clip_position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return out;
}
@group(0) @binding(0) var light_buffer: texture_2d<f32>;
@group(0) @binding(1) var light_sampler: sampler;
struct PresentUniform { tone_mode: u32, }
@group(0) @binding(2) var<uniform> present_uniform: PresentUniform; // tone_mode: 0 = Reinhard, 1 = None
fn tonemap_reinhard(c: vec3<f32>) -> vec3<f32> { return c / (1.0 + c); }
fn tonemap_none(c: vec3<f32>) -> vec3<f32> { return clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)); }
@fragment fn fs(in: VertexOutput) -> @location(0) vec4<f32> {
    let hdr = textureSample(light_buffer, light_sampler, in.uv);
    let ldr_rgb = select(tonemap_none(hdr.rgb), tonemap_reinhard(hdr.rgb), present_uniform.tone_mode == 0u);
    return vec4<f32>(ldr_rgb, 1.0);
}
