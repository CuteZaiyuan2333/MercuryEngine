# Lumelite 当前开发状态

本文档基于 `lumelite/` 实际代码，总结**当前已具备的能力**与**可选后续工作**。Lumelite 采用 **wgpu** 作为 RHI，与 Lume（lume-rhi/Vulkan）相互独立；通过 **render-api** 与 **RenderBackend** 与 Lume 接口对齐。

---

## 1. 已具备能力

### 1.1 lumelite-renderer

- **RHI**：直接使用 **wgpu**（无 lume-rhi），着色器为 **WGSL**。
- **已实现**：
  - **GBuffer Pass**：多 RT + Depth，输出供 Light Pass 使用。
  - **Light Pass**：方向光全屏、延迟光照（Lambert + GGX/Schlick/Smith）。
  - **Present Pass**：Light Buffer → 交换链或离屏目标，含色调映射。
  - **Render Graph 概念**：Pass 依赖、资源生命周期、FrameResources（GBuffer、Light Buffer、Depth 等）。
- **对外 API**：`Renderer::new_with_config(device, queue, config)`、`encode_frame(...)`、`encode_present_to(...)`、`ensure_frame_resources`、`submit` 等；支持 `MeshDraw`（vertex_buf、index_buf、index_count、transform）。

### 1.2 lumelite-bridge

- **类型**：不单独定义 Extract 类型；依赖根目录 **render-api**，与 Lume 共用 `ExtractedMesh`、`ExtractedMeshes`、`ExtractedView`。
- **RenderBackend 实现**：**LumelitePlugin** 完整实现 `render_api::RenderBackend`：
  - **prepare(&mut self, extracted: &ExtractedMeshes)**：遍历 `extracted.meshes`，对可见 Mesh 创建 wgpu Buffer（顶点/索引），写入数据并存入 `mesh_cache`（按 entity_id 缓存）。
  - **render_frame(&mut self, view: &ExtractedView)**：从 `mesh_cache` 构建 `MeshDraw` 列表，调用 `lumelite_renderer::Renderer::encode_frame`（GBuffer + Light Pass），内部 submit；离屏一帧闭环。
- **窗口输出**：`render_frame_to_swapchain(&mut self, view, swapchain_view)`：在同一帧内完成 encode_frame + encode_present_to，并 submit，将结果呈现到 swapchain。

### 1.3 示例程序（debug/）

示例已迁移至仓库根目录 **debug/**，与 lumelite workspace 分离，通过 path 依赖 `lumelite-renderer`、`lumelite-bridge`、`render-api` 使用：

- **minimal_wgpu**：无窗口，最小 wgpu 初始化（request_adapter、request_device），验证 wgpu 可用。运行：在仓库根目录 `cargo run -p debug --bin minimal_wgpu`，或 `cd debug && cargo run --bin minimal_wgpu`。
- **plugin_loop**：使用 `render_api::RenderBackend` + `LumelitePlugin`，单帧 `prepare(ExtractedMeshes)` + `render_frame(ExtractedView)` 离屏闭环，验证 Bridge 与 Renderer 联动。运行：在仓库根目录 `cargo run -p debug --bin plugin_loop`，或 `cd debug && cargo run --bin plugin_loop`。
- **gbuffer_light_window**：使用 `LumeliteWindowBackend`（后端无关），窗口 + 每帧 `prepare` + `render_frame_to_window`（内部处理 swapchain），可看到 GBuffer + 方向光光照的三角形。运行：在仓库根目录 `cargo run -p debug --bin gbuffer_light_window`，或 `cd debug && cargo run --bin gbuffer_light_window`。

### 1.4 render-api（仓库根目录）

- **定义**：`ExtractedMesh`、`ExtractedMeshes`、`ExtractedView`、`RenderBackend` trait（`prepare`、`render_frame`）。
- **用途**：宿主与 Lume/Lumelite 共用同一套类型与调用方式；lume-bridge 与 lumelite-bridge 均实现 `RenderBackend`，可互换。

---

## 2. 与 Lume 的关系（简要）

| 项目     | Lume                         | Lumelite                          |
|----------|------------------------------|------------------------------------|
| RHI      | lume-rhi（Vulkan）           | **wgpu**（无 lume-rhi）            |
| Renderer | lume-renderer（Graph、VG/GI 占位） | lumelite-renderer（GBuffer + Light + Present） |
| Bridge   | LumePlugin（prepare 为 TODO） | **LumelitePlugin**（prepare + render_frame 已闭环） |
| 接口     | render-api 类型 + RenderBackend | 同上                               |

Lumelite 与 Lume **平行**：不共用 RHI 或 Renderer 源码，仅通过 render-api 在类型与 RenderBackend 上对齐，便于宿主切换后端。

---

## 3. 可选后续工作

以下为在现有闭环基础上的增强，非必须：

| 方向         | 说明 |
|--------------|------|
| 多光源       | 点光、聚光、天光等（lumelite-renderer 已有 Light Pass 结构，可扩展光源数据与着色器）。 |
| Resize 处理  | 窗口缩放时 swapchain 与 FrameResources 的 resize/重建，避免 DEVICE_LOST。 |
| 阴影         | 可选的 Shadow Map Pass（单光源、单 cascade）。 |
| 示例独立化   | **已完成**：示例已迁移至仓库根目录 **debug/**，通过 path 依赖 lumelite-* 与 render-api 使用。 |

---

## 4. 小结

- **lumelite-renderer**：基于 wgpu 的 GBuffer + Light + Present 管线已就绪。
- **lumelite-bridge**：LumelitePlugin 完整实现 RenderBackend，prepare 与 render_frame（含 swapchain）闭环已完成。
- **示例**：位于 **debug/** 的 minimal_wgpu、plugin_loop、gbuffer_light_window 可验证从初始化到窗口渲染的完整流程。
- **与 Lume**：通过 render-api 统一类型与 RenderBackend；Lumelite 为当前**完全可用**的后端，Lume 为**有良好基础、未完成闭环**的后端，两者可并行使用或切换。
