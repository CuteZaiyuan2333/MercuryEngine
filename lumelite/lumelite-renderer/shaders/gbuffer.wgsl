// Flax-style PBR GBuffer: position+normal+uv (stride 32), sample base_color, normal, metallic_roughness, ao.

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world_pos: vec3<f32>,
}

@group(0) @binding(0) var<uniform> view_proj: mat4x4<f32>;
@group(0) @binding(1) var<uniform> model: mat4x4<f32>;

@group(1) @binding(0) var base_color_tex: texture_2d<f32>;
@group(1) @binding(1) var normal_tex: texture_2d<f32>;
@group(1) @binding(2) var metallic_roughness_tex: texture_2d<f32>;
@group(1) @binding(3) var ao_tex: texture_2d<f32>;
@group(1) @binding(4) var tex_sampler: sampler;

@vertex fn vs(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = (model * vec4<f32>(in.position, 1.0)).xyz;
    out.clip_position = view_proj * vec4<f32>(world_pos, 1.0);
    out.world_normal = (model * vec4<f32>(in.normal, 0.0)).xyz;
    out.uv = in.uv;
    out.world_pos = world_pos;
    return out;
}

fn encode_normal(n: vec3<f32>) -> vec3<f32> {
    return n * 0.5 + 0.5;
}

// Unpack tangent-space normal from RGBA; z from xy
fn unpack_normal_ts(enc: vec3<f32>) -> vec3<f32> {
    let n = enc * 2.0 - 1.0;
    let z = sqrt(max(0.0, 1.0 - dot(n.xy, n.xy)));
    return vec3<f32>(n.xy, z);
}

// TBN from world_normal and uv derivatives (simplified: use world_normal as basis for flat shading when normal map is neutral)
fn tangent_from_world_normal(world_normal: vec3<f32>) -> vec3<f32> {
    let n = normalize(world_normal);
    if abs(n.z) < 0.999 {
        return normalize(cross(vec3<f32>(0.0, 0.0, 1.0), n));
    }
    return normalize(cross(vec3<f32>(0.0, 1.0, 0.0), n));
}

struct FragmentOutput {
    @location(0) gbuffer0: vec4<f32>,
    @location(1) gbuffer1: vec4<f32>,
    @location(2) gbuffer2: vec4<f32>,
    @location(3) gbuffer3: vec4<f32>,
}

@fragment fn fs(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;
    let base_color = textureSample(base_color_tex, tex_sampler, in.uv).rgb;
    let ao_val = textureSample(ao_tex, tex_sampler, in.uv).r;
    let mr = textureSample(metallic_roughness_tex, tex_sampler, in.uv);
    let roughness = max(mr.g, 0.04);
    let metalness = mr.r;
    let specular_val = 0.5;

    let n_ts = unpack_normal_ts(textureSample(normal_tex, tex_sampler, in.uv).rgb);
    let tangent = tangent_from_world_normal(in.world_normal);
    let bitangent = cross(in.world_normal, tangent);
    let tbn = mat3x3<f32>(tangent, bitangent, normalize(in.world_normal));
    let world_normal = normalize(tbn * n_ts);

    out.gbuffer0 = vec4<f32>(base_color, ao_val);
    out.gbuffer1 = vec4<f32>(encode_normal(world_normal), 1.0 / 3.0);
    out.gbuffer2 = vec4<f32>(roughness, metalness, specular_val, 0.0);
    out.gbuffer3 = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    return out;
}
