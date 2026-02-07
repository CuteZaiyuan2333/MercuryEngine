# 延迟光照与 GBuffer 方案分析

本文档总结常见的 Deferred 光照算法与管线结构（GBuffer + Light Pass），以及 Lumelite 采用 **wgpu**（不使用 Vulkan/Metal 直连）时的实现要点。以下管线与数据结构可作为实现参考。

---

## 1. 参考实现中的渲染与光照代码位置概览

| 类别 | 路径 | 说明 |
|------|------|------|
| 光照 Pass | `Source/Engine/Renderer/LightPass.h`, `LightPass.cpp` | 光照渲染服务：动态光漫反射与高光计算 |
| 光照着色器 | `Source/Shaders/Lights.shader` | 方向光/点光/聚光/天光 PS 入口 |
| 光照算法 | `Source/Shaders/Lighting.hlsl`, `LightingCommon.hlsl` | GetLighting、StandardShading、BRDF、衰减 |
| BRDF | `Source/Shaders/BRDF.hlsl` | Lambert、GGX、Schlick、Smith 等 |
| GBuffer | `Source/Shaders/GBuffer.hlsl`, `GBufferCommon.hlsl` | 布局、SampleGBuffer、GetDiffuseColor/GetSpecularColor |
| GBuffer Pass | `Source/Engine/Renderer/GBufferPass.h`, `GBufferPass.cpp` | 填充 GBuffer、SetInputs |
| 光源数据 | `Source/Engine/Renderer/RenderList.h` | RenderLightData、RenderDirectionalLightData、RenderPointLightData、RenderSpotLightData、SkyLight |
| 阴影 | `Source/Engine/Renderer/ShadowsPass.*`, `Source/Shaders/Shadows*.hlsl` | 阴影图与 Shadow Mask |
| 管线顺序 | `Source/Engine/Renderer/Renderer.cpp` | GBufferPass::Fill → LightPass::RenderLights → ... |

参考实现使用 Vulkan/DirectX 等原生 API，**未使用 wgpu**。Lumelite 仅借鉴**光照算法与管线结构**，在 Rust + wgpu + WGSL 中独立实现。

---

## 2. 光照算法要点（可于 WGSL/wgpu 中实现）

### 2.1 管线结构

- **Deferred 路径**：先 **GBuffer Pass** 填充多 RT（见下），再 **Light Pass** 对屏幕或光源几何做光照累加。
- **Light Pass 输入**：GBuffer0～3、Depth、Shadow Mask（可选）；**输出**：Light Buffer（与 GBuffer0 共用或单独，加性混合）。
- **光源顺序**：Point → Spot → Directional → Sky（可按屏幕占比与亮度排序后依次绘制）。

### 2.2 GBuffer 布局（与 Lighting 对接）

- **GBuffer0**：RGB = Base Color，A = AO  
- **GBuffer1**：RGB = Normal（EncodeNormal 压缩），A = ShadingModel  
- **GBuffer2**：R = Roughness，G = Metalness，B = Specular，A = 未用  
- **GBuffer3**：RGBA = CustomData（如 Subsurface/Foliage）  
- **Depth**：单独深度缓冲，用于重建 ViewPos/WorldPos  

材质漫反射/高光色由 Filament 式公式从 Color/Metalness/Specular 得到（见 `GBufferCommon.hlsl` 的 `GetDiffuseColor` / `GetSpecularColor`）。

### 2.3 光照数据结构（LightData，与 LightingCommon.hlsl 一致）

- SpotAngles、SourceRadius、SourceLength、Color、MinRoughness  
- Position、ShadowsBufferAddress、Direction、Radius  
- FalloffExponent、InverseSquared、RadiusInv  

在 Lumelite 中可用同一布局的 uniform 或 buffer，便于与光照计算逻辑对接。

### 2.4 光照计算流程（GetLighting）

1. **阴影**：`GetShadow(lightData, gBuffer, shadowMask)` → SurfaceShadow = AO * shadowMask.r，TransmissionShadow = shadowMask.g。  
2. **径向光衰减**：`GetRadialLightAttenuation`（距离：InverseSquared 时为 `1/(d^2+bias)` 或 FalloffExponent 的平滑衰减；聚光：SpotAngles 锥体衰减）。  
3. **方向光**：Direction 固定，NoL 参与阴影。  
4. **高光能量**：`AreaLightSpecular`（线光源/面光源的代表方向与能量修正）。  
5. **表面着色**：`SurfaceShading` → `StandardShading`（默认）：  
   - **漫反射**：Lambert（`Diffuse_Lambert(diffuseColor)`）。  
   - **高光**：GGX D、Schlick F、Smith Joint 近似 V，`(D*Vis)*F`。  
6. **可选**：SubsurfaceShading、FoliageShading（依赖 GBuffer CustomData）。  
7. **天光**：`GetSkyLightLighting`：用法线或固定方向采样 IBL 立方体贴图，按到捕获位置距离淡出。

以上均为标量/向量运算与纹理采样，**与图形 API 无关**，适合用 WGSL 在 wgpu 中重写。

### 2.5 Light Pass 绘制方式

- **方向光**：全屏三角形，PS 里采样 GBuffer + Shadow，调用光照计算（方向光路径）。  
- **点光/聚光/天光**：用球体 Mesh 绘制，VS 里做 WVP 变换，PS 里用 ScreenPos 反算 UV 再采样 GBuffer；Inside 时用 DepthFunc::Greater + CullMode::Inverted。  
- **混合**：Additive，写 RGB。  
- **可选**：Depth Bounds、Shadow Mask 逐光源渲染、IES 纹理乘数。

Lumelite 在 wgpu 中可采用相同策略：全屏 quad + 球体 mesh，加性混合，按需简化（如先不做 IES、不做 Depth Bounds）。

---

## 3. 与 Lumelite 的适配结论

**可行。** 理由简要如下：

| 维度 | 说明 |
|------|------|
| **算法独立性** | 光照与 BRDF 仅用标准数学与纹理采样，可在 WGSL 中完整实现。 |
| **管线清晰** | GBuffer → Light Pass（加性）→ 后处理，与 Lumelite 的「无 VG/GI」轻量管线一致。 |
| **复杂度可控** | 先实现：GBuffer（含 Normal/Roughness/Metalness/Specular）+ 方向光/点光/聚光/天光 + Lambert + GGX/Schlick/Smith；Subsurface/Foliage/IES 可后加或省略。 |
| **阴影** | 复杂 Shadow Mask 可后续再做；Lumelite 可先做「无阴影」或「单级 CSM 简单阴影」。 |
| **与 wgpu 的匹配** | wgpu 支持 MRT、深度、加性混合、uniform/buffer、纹理采样，满足 Light Pass 与 GBuffer 所需能力。 |

在 Lumelite 内用 **Rust + wgpu + WGSL** 独立实现算法与管线即可，无需依赖外部 C++/Vulkan/DX 代码。

---

## 4. Lumelite 使用 wgpu 而非 Vulkan/Metal 的架构影响

您计划在 Lumelite 中**不使用 Vulkan 或 Metal，而直接使用 wgpu**。这与当前 Docs/lumelite 中「与 Lume 共用 lume-rhi」的假设不一致，需要明确调整。

### 4.1 Lumelite 技术选型

- **RHI / 设备层**：**wgpu**（Rust 的 `wgpu` crate），不依赖 Lume 的 `lume-rhi`（Vulkan/Metal）。  
- **着色器**：**WGSL**，实现 Lighting、BRDF、GBuffer、Lights 等，算法采用标准公式（Lambert、GGX/Schlick/Smith 等）。  
- **光照与管线**：GBuffer Pass → Light Pass（方向/点/聚/天光，加性）→ 可选后处理；数据结构（如 LightData、GBuffer 布局）与管线约定一致。  
- **与 Lume 的兼容**：在**高层**保持兼容：Extract 数据类型（ExtractedMesh、ExtractedView 等）、Render Graph 的**概念**（Pass 依赖、资源生命周期）、Bridge 的 `prepare` / `render_frame` **语义**；**RHI 层**使用 wgpu，不与 Lume 共用。

### 4.2 与 Lume 的差异（采用 wgpu 后）

| 项目 | Lume | Lumelite |
|------|------|---------------------|
| 图形 API | lume-rhi（Vulkan，预留 Metal） | wgpu（Vulkan/Metal/DX12 等由 wgpu 选择） |
| 着色器 | SPIR-V（Vulkan） | WGSL |
| 光照模型 | 计划 Lumen 式 GI | Deferred + 多光源（方向/点/聚/天光） |
| 管线 | GBuffer + VG + GI 等 | GBuffer + Light Pass，无 VG/GI |

Lumelite 与 Lume 在「数据流与 Bridge 接口」上对齐，便于在 MercuryEngine 中切换或共存；底层实现为 wgpu + 延迟光照。

---

## 5. 实施要点（摘要）

1. **GBuffer**：实现多 RT + Depth（Color+AO、Normal、Roughness/Metalness/Specular、CustomData 等）；材质输出 Color、Normal、Roughness、Metalness、Specular、AO、ShadingModel、可选 CustomData。  
2. **LightData**：在 Rust 侧定义光源数据结构，通过 wgpu buffer 传入；支持方向/点/聚/天光。  
3. **BRDF 与光照计算**：在 WGSL 中实现 Diffuse_Lambert、D_GGX、F_Schlick、Vis_SmithJointApprox、径向光衰减、AreaLightSpecular、GetLighting、GetSkyLightLighting 等。  
4. **Lights Pass**：方向光用 fullscreen quad；点/聚/天光用球体 mesh；加性混合；每光源绑定 LightData 与可选 Shadow Mask；无阴影时可传 1。  
5. **Render Graph**：在现有「图 + 资源」概念下，增加 GBuffer Pass 节点与 Light Pass 节点，Light Pass 依赖 GBuffer 与 Depth，输出 Light Buffer；与 Bridge 的 prepare/render_frame 衔接。  
6. **阴影**：首版可省略或只做简单方向光阴影；后续再引入 Shadow Mask 等进阶思路。

按上述做法，Lumelite 的延迟光照管线在技术与选型上可行，且与「使用 wgpu、不用 Vulkan/Metal 直连」的决策一致。
