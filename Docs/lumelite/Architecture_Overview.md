# Lumelite 渲染引擎架构设计文档

## 1. 定位与目标

Lumelite 是 **Lume 的轻量兼容子集**，作为 MercuryEngine 的**基础渲染后端**，在**高层接口与数据流**上与 Lume 对齐，但采用**独立技术选型**：

- **不包含**：虚拟几何体 (Virtual Geometry)、GPU 驱动裁剪、Cluster 流送、软光栅化
- **不包含**：全局光照 (Global Illumination)、SDF 光线追踪、Surface Cache、时域降噪
- **不包含**：lume-tools 中的 Cluster 预处理、Mesh SDF 烘焙等离线管线

**技术选型（与 Lume 分叉）**：

- **RHI**：**wgpu**（Rust 的 `wgpu` crate），**不使用** Vulkan/Metal 直连或 Lume 的 `lume-rhi`。着色器使用 **WGSL**。
- **光照**：Deferred 光照算法（GBuffer + 多光源 Light Pass、Lambert + GGX/Schlick/Smith、方向/点/聚/天光）。详见 [Flax_Lighting_Analysis.md](Flax_Lighting_Analysis.md)。

**目标**：

- 与 Lume **高层兼容**：共用 **render-api** 类型（`ExtractedMesh`、`ExtractedView`）与 **RenderBackend** trait（prepare/render_frame）；Lumelite 的 Bridge 实现为 `LumelitePlugin`（与 Lume 的 `LumePlugin` 接口对齐，构造参数不同）。数据流（Extract → Prepare → Render）一致；Render Graph **概念**（Pass 依赖、资源生命周期）一致；**RHI 层**则使用 wgpu，不与 Lume 共用。
- **实现范围**：基于 wgpu 的 GBuffer + Light Pass、Render Graph、基础 Mesh 渲染、与 MercuryEngine 的 Prepare/Render 对接。

## 2. 分层架构图

```
┌─────────────────────────────────────────────────────────────────┐
│  Mercury Bridge (High-level API)                                 │
│  RenderBackend, ExtractedMesh/ExtractedView, prepare/render_frame │
│  类型与语义由 render-api 统一；LumelitePlugin 实现 Prepare（Mesh→Buffer）│
└─────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────┐
│  Lumelite Renderer                                                │
│  • Render Graph：Pass 依赖、资源生命周期（概念与 Lume 一致）      │
│  • Shadow Pass → GBuffer Pass → Light Pass（多光源、加性混合）→ Present │
│  • 点光/聚光、Shadow Map（单 cascade）、uniform 复用；无 VG/GI    │
└─────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────┐
│  wgpu（不共用 Lume RHI）                                           │
│  Rust wgpu crate：Device, Queue, Buffer, Texture, RenderPipeline  │
│  WGSL 着色器；wgpu 负责 Vulkan/Metal/DX12 等后端                   │
└─────────────────────────────────────────────────────────────────┘
```

### 2.1 wgpu（RHI 层，与 Lume 分叉）

- **职责**：使用 **wgpu** 作为唯一图形 API，不再使用 Lume 的 `lume-rhi`（Vulkan/Metal）。
- **Lumelite 使用方式**：直接依赖 `wgpu` crate；所有管线、缓冲、纹理、同步均通过 wgpu 的 Device/Queue/RenderPass 等完成。
- **着色器**：**WGSL**；光照与 BRDF 使用 Lambert + GGX/Schlick/Smith 等标准算法。

### 2.2 Lumelite Renderer（延迟光照 + Render Graph 概念）

- **保留（概念与高层 API 与 Lume 对齐）**：
  - **Render Graph**：`lumelite-renderer` 的 `graph` 模块提供 RenderGraph、RenderGraphNode 等概念型实现，用于资源依赖与拓扑排序。当前主渲染路径（encode_frame）仍按固定顺序直接调用 GBuffer → Light → Present，未接入 Render Graph。Render Graph 预留供后续扩展使用。
  - **Renderer**：对外仍可暴露 `render_frame()` 等入口，返回由 wgpu 录制的命令（或直接在本进程内 submit）。
- **光照管线**：
  - **Shadow Pass**：方向光视角深度渲染至 shadow map；`LumeliteConfig::shadow_enabled` 控制；单 cascade。
  - **GBuffer Pass**：多 RT（Color+AO、Normal+ShadingModel、Roughness/Metalness/Specular、CustomData）+ Depth；view_proj uniform 复用。
  - **Light Pass**：方向光全屏；点光、聚光全屏 + 距离/锥体衰减；加性混合；BRDF 为 Lambert + GGX/Schlick/Smith；ExtractedView.point_lights、spot_lights。详见 [Flax_Lighting_Analysis.md](Flax_Lighting_Analysis.md)。
- **移除或占位**：virtual_geom、gi 不实现。

### 2.3 Mercury Bridge（与 Lume 接口一致）

- **类型**：`ExtractedMesh`、`ExtractedMeshes`、`ExtractedView` 由 **render-api** 定义，Lume 与 Lumelite 共用。
- **插件**：Lumelite 暴露 `LumelitePlugin`，实现 `render_api::RenderBackend`（`prepare(extracted)`、`render_frame(view)`）；内部使用 wgpu 与 Lumelite Renderer（GBuffer + Light Pass）。构造为 `LumelitePlugin::new(device, queue)` 或 `new_with_config(...)`。
- **Lumelite 实现重点**：Prepare 中创建/更新 wgpu Buffer（顶点/索引）并缓存；render_frame 构建并执行 GBuffer → Light Pass，支持离屏或 `render_frame_to_swapchain` 与 Swapchain 衔接。

## 3. 与 Lume 的差异总结

| 项目           | Lume                         | Lumelite（wgpu + 延迟光照）             |
|----------------|------------------------------|----------------------------------------|
| RHI            | lume-rhi（Vulkan，预留 Metal）| **wgpu**（Vulkan/Metal/DX12 等由 wgpu 选） |
| 着色器         | SPIR-V                       | **WGSL**                               |
| Render Graph   | 完整                         | 概念一致，实现基于 wgpu                |
| 光照           | 计划 Lumen 式 GI             | **Deferred（GBuffer + Light Pass）**   |
| 虚拟几何体     | 有                           | 无                                     |
| 全局光照       | 有                           | 无                                     |
| Bridge 类型/API| 一致                         | 一致                                   |

## 4. 可行性评估

**可行性：高**。  
延迟光照算法与管线与图形 API 解耦，可在 WGSL + wgpu 中完整实现；与 Lume 在 Bridge 与数据流上的兼容保留，仅 RHI 层改为 wgpu。详见 [Flax_Lighting_Analysis.md](Flax_Lighting_Analysis.md)。
