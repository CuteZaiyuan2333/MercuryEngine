struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_fullscreen(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    out.uv = vec2<f32>(x, y);
    out.clip_position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    return out;
}
@group(0) @binding(0) var gbuffer0: texture_2d<f32>;
@group(0) @binding(1) var gbuffer1: texture_2d<f32>;
@group(0) @binding(2) var gbuffer2: texture_2d<f32>;
@group(0) @binding(3) var depth_tex: texture_depth_2d;
@group(0) @binding(4) var gbuffer_sampler: sampler;
struct LightUniform {
    direction: vec3<f32>,
    _pad0: f32,
    color: vec3<f32>,
    _pad1: f32,
    inv_view_proj: mat4x4<f32>,
}
@group(0) @binding(5) var<uniform> light: LightUniform;

fn decode_normal(enc: vec3<f32>) -> vec3<f32> { return normalize(enc * 2.0 - 1.0); }
const PI: f32 = 3.14159265359;

// ——— Flax BRDF (Source/Shaders/BRDF.hlsl, Lighting.hlsl, GBufferCommon.hlsl) ———

// Diffuse_Lambert: returns diffuseColor * (1/PI); NdotL applied in lighting.
fn Diffuse_Lambert(diffuse_color: vec3<f32>) -> vec3<f32> {
    return diffuse_color * (1.0 / PI);
}

// D_GGX [Walter et al. 2007] — roughness, NoH
fn D_GGX(roughness: f32, n_dot_h: f32) -> f32 {
    let a = roughness * roughness;
    let a2 = a * a;
    let d = (n_dot_h * a2 - n_dot_h) * n_dot_h + 1.0;
    return a2 / (PI * d * d);
}

// F_Schlick [Schlick 1994]
fn F_Schlick(specular_color: vec3<f32>, v_dot_h: f32) -> vec3<f32> {
    let fc = pow(1.0 - max(v_dot_h, 0.0), 5.0);
    return fc + (1.0 - fc) * specular_color;
}

// Vis_SmithJointApprox [Heitz 2014] — Flax uses this for (D*Vis)*F
fn Vis_SmithJointApprox(roughness: f32, n_dot_v: f32, n_dot_l: f32) -> f32 {
    let a = roughness * roughness;
    let vis_smith_v = n_dot_l * (n_dot_v * (1.0 - a) + a);
    let vis_smith_l = n_dot_v * (n_dot_l * (1.0 - a) + a);
    return 0.5 / (vis_smith_v + vis_smith_l);
}

// GetDiffuseColor / GetSpecularColor (GBufferCommon.hlsl, Filament-style)
fn GetDiffuseColor(color: vec3<f32>, metalness: f32) -> vec3<f32> {
    return color * (1.0 - metalness);
}
fn GetSpecularColor(color: vec3<f32>, specular: f32, metalness: f32) -> vec3<f32> {
    let dielectric_f0 = 0.16 * specular * specular;
    return mix(vec3<f32>(dielectric_f0, dielectric_f0, dielectric_f0), color, vec3<f32>(metalness, metalness, metalness));
}

@fragment fn fs_directional(in: VertexOutput) -> @location(0) vec4<f32> {
    let g0 = textureSample(gbuffer0, gbuffer_sampler, in.uv);
    let g1 = textureSample(gbuffer1, gbuffer_sampler, in.uv);
    let g2 = textureSample(gbuffer2, gbuffer_sampler, in.uv);
    let dims = vec2<f32>(textureDimensions(depth_tex));
    let pix = vec2<i32>(min(floor(in.uv * dims), dims - vec2<f32>(1.0, 1.0)));
    let depth_val = textureLoad(depth_tex, pix, 0);
    if depth_val >= 1.0 { return vec4<f32>(0.0, 0.0, 0.0, 0.0); }

    let n = decode_normal(g1.rgb);
    let roughness = max(g2.r, 0.04);
    let metalness = g2.g;
    let specular_val = g2.b;
    let base_color = g0.rgb;
    let ao = g0.a;

    // Reconstruct world position from depth and NDC
    let ndc = vec4<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0, depth_val, 1.0);
    let world_h = light.inv_view_proj * ndc;
    let world_pos = world_h.xyz / world_h.w;
    let cam_col = light.inv_view_proj * vec4<f32>(0.0, 0.0, 0.0, 1.0);
    let camera_pos = cam_col.xyz / cam_col.w;
    let v = normalize(camera_pos - world_pos);

    // direction = where light shines (from light toward scene); l = toward light (from surface)
    let l = -normalize(light.direction);
    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 1e-5);
    let h = normalize(v + l);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);

    // StandardShading: GetDiffuseColor / GetSpecularColor (Flax GBufferCommon)
    let diffuse_color = GetDiffuseColor(base_color, metalness);
    let specular_color = GetSpecularColor(base_color, specular_val, metalness);

    // Diffuse: Diffuse_Lambert(diffuseColor) * lightColor * NoL * AO
    var lit = Diffuse_Lambert(diffuse_color) * light.color * n_dot_l * ao;

    // Specular: (D * Vis) * F [Flax StandardShading]; energy = 1 for directional
    let energy = 1.0;
    let D = D_GGX(roughness, n_dot_h) * energy;
    let Vis = Vis_SmithJointApprox(roughness, n_dot_v, n_dot_l);
    let F = F_Schlick(specular_color, v_dot_h);
    lit += (D * Vis) * F * light.color * n_dot_l;

    return vec4<f32>(lit, 1.0);
}

// Point light: fullscreen, attenuation by distance
struct PointLightUniform {
    position: vec3<f32>,
    _pad0: f32,
    color: vec3<f32>,
    _pad1: f32,
    radius: f32,
    falloff_exponent: f32,
    _pad2: vec2<f32>,
    inv_view_proj: mat4x4<f32>,
}
@group(0) @binding(5) var<uniform> point_light: PointLightUniform;

fn GetRadialLightAttenuation(dist: f32, radius: f32, falloff: f32) -> f32 {
    let t = 1.0 - clamp(dist / radius, 0.0, 1.0);
    return pow(t, falloff);
}

@fragment fn fs_point(in: VertexOutput) -> @location(0) vec4<f32> {
    let g0 = textureSample(gbuffer0, gbuffer_sampler, in.uv);
    let g1 = textureSample(gbuffer1, gbuffer_sampler, in.uv);
    let g2 = textureSample(gbuffer2, gbuffer_sampler, in.uv);
    let dims = vec2<f32>(textureDimensions(depth_tex));
    let pix = vec2<i32>(min(floor(in.uv * dims), dims - vec2<f32>(1.0, 1.0)));
    let depth_val = textureLoad(depth_tex, pix, 0);
    if depth_val >= 1.0 { return vec4<f32>(0.0, 0.0, 0.0, 0.0); }

    let n = decode_normal(g1.rgb);
    let roughness = max(g2.r, 0.04);
    let metalness = g2.g;
    let specular_val = g2.b;
    let base_color = g0.rgb;
    let ao = g0.a;

    let ndc = vec4<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0, depth_val, 1.0);
    let world_h = point_light.inv_view_proj * ndc;
    let world_pos = world_h.xyz / world_h.w;
    let cam_col = point_light.inv_view_proj * vec4<f32>(0.0, 0.0, 0.0, 1.0);
    let camera_pos = cam_col.xyz / cam_col.w;
    let v = normalize(camera_pos - world_pos);

    let to_light = point_light.position - world_pos;
    let dist = length(to_light);
    let l = normalize(to_light);
    let attenuation = GetRadialLightAttenuation(dist, point_light.radius, point_light.falloff_exponent);
    if attenuation <= 0.0 { return vec4<f32>(0.0, 0.0, 0.0, 0.0); }

    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 1e-5);
    let h = normalize(v + l);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);

    let diffuse_color = GetDiffuseColor(base_color, metalness);
    let specular_color = GetSpecularColor(base_color, specular_val, metalness);

    var lit = Diffuse_Lambert(diffuse_color) * point_light.color * n_dot_l * ao * attenuation;
    let D = D_GGX(roughness, n_dot_h);
    let Vis = Vis_SmithJointApprox(roughness, n_dot_v, n_dot_l);
    let F = F_Schlick(specular_color, v_dot_h);
    lit += (D * Vis) * F * point_light.color * n_dot_l * attenuation;

    return vec4<f32>(lit, 1.0);
}

// Spot light: fullscreen, attenuation by distance + cone
struct SpotLightUniform {
    position: vec3<f32>,
    _pad0: f32,
    direction: vec3<f32>,
    _pad1: f32,
    color: vec3<f32>,
    _pad2: f32,
    radius: f32,
    inner_cos: f32,
    outer_cos: f32,
    _pad3: f32,
    inv_view_proj: mat4x4<f32>,
}
@group(0) @binding(5) var<uniform> spot_light: SpotLightUniform;

fn GetSpotConeAttenuation(l_dir: vec3<f32>, spot_dir: vec3<f32>, inner_cos: f32, outer_cos: f32) -> f32 {
    let cos_angle = dot(-l_dir, spot_dir);
    return smoothstep(outer_cos, inner_cos, cos_angle);
}

@fragment fn fs_spot(in: VertexOutput) -> @location(0) vec4<f32> {
    let g0 = textureSample(gbuffer0, gbuffer_sampler, in.uv);
    let g1 = textureSample(gbuffer1, gbuffer_sampler, in.uv);
    let g2 = textureSample(gbuffer2, gbuffer_sampler, in.uv);
    let dims = vec2<f32>(textureDimensions(depth_tex));
    let pix = vec2<i32>(min(floor(in.uv * dims), dims - vec2<f32>(1.0, 1.0)));
    let depth_val = textureLoad(depth_tex, pix, 0);
    if depth_val >= 1.0 { return vec4<f32>(0.0, 0.0, 0.0, 0.0); }

    let n = decode_normal(g1.rgb);
    let roughness = max(g2.r, 0.04);
    let metalness = g2.g;
    let specular_val = g2.b;
    let base_color = g0.rgb;
    let ao = g0.a;

    let ndc = vec4<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0, depth_val, 1.0);
    let world_h = spot_light.inv_view_proj * ndc;
    let world_pos = world_h.xyz / world_h.w;
    let cam_col = spot_light.inv_view_proj * vec4<f32>(0.0, 0.0, 0.0, 1.0);
    let camera_pos = cam_col.xyz / cam_col.w;
    let v = normalize(camera_pos - world_pos);

    let to_light = spot_light.position - world_pos;
    let dist = length(to_light);
    let l = normalize(to_light);
    let radial_atten = GetRadialLightAttenuation(dist, spot_light.radius, 2.0);
    let cone_atten = GetSpotConeAttenuation(l, spot_light.direction, spot_light.inner_cos, spot_light.outer_cos);
    let attenuation = radial_atten * cone_atten;
    if attenuation <= 0.0 { return vec4<f32>(0.0, 0.0, 0.0, 0.0); }

    let n_dot_l = max(dot(n, l), 0.0);
    let n_dot_v = max(dot(n, v), 1e-5);
    let h = normalize(v + l);
    let n_dot_h = max(dot(n, h), 0.0);
    let v_dot_h = max(dot(v, h), 0.0);

    let diffuse_color = GetDiffuseColor(base_color, metalness);
    let specular_color = GetSpecularColor(base_color, specular_val, metalness);

    var lit = Diffuse_Lambert(diffuse_color) * spot_light.color * n_dot_l * ao * attenuation;
    let D = D_GGX(roughness, n_dot_h);
    let Vis = Vis_SmithJointApprox(roughness, n_dot_v, n_dot_l);
    let F = F_Schlick(specular_color, v_dot_h);
    lit += (D * Vis) * F * spot_light.color * n_dot_l * attenuation;

    return vec4<f32>(lit, 1.0);
}
