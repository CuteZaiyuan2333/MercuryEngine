# Lumelite 与 Lume 接口兼容说明

本文档明确 Lumelite 与 Lume 在**类型、API、数据流**上的兼容点与差异，以及从 Lumelite 迁移到 Lume 的推荐步骤。

---

## 1. 完全兼容的部分（可直接复用或同源）

### 1.1 数据提取类型（render-api，与 Lume 共用）

| 类型 | 说明 | 兼容性 |
|------|------|--------|
| `ExtractedMesh` | entity_id, vertex_data, index_data, transform, visible | 定义在 **render-api**，Lume 与 Lumelite 共用；Lumelite 使用 vertex_data/index_data/transform，visible 参与剔除。**transform** 为列主序 4x4 矩阵（与 WGSL `mat4x4<f32>` 一致） |
| `ExtractedMeshes` | meshes: HashMap<u64, ExtractedMesh> | 一致 |
| `ExtractedView` | view_proj, viewport_size, directional_light, point_lights, spot_lights, sky_light | 一致；**directional_light** 可选；**point_lights**、**spot_lights** 为 `Vec`，Lumelite 按 `LumeliteConfig::max_*` 截断；**sky_light** 结构已预留 |

**统一 API**：类型由仓库根目录 **render-api** crate 定义；lume-bridge 与 lumelite-bridge 均依赖 render-api 并实现 **RenderBackend** trait（`prepare`、`render_frame`）。宿主只依赖 render-api，可同一套 Extract 逻辑与同一套 prepare/render_frame 调用对接 Lume 或 Lumelite。详见 [Backend_Switch.md](Backend_Switch.md)。

### 1.2 RenderBackend 与 Bridge 对外 API

| 项目 | Lume | Lumelite |
|------|------|----------|
| 类型名 | `lume_bridge::LumePlugin` | `lumelite_bridge::LumelitePlugin` |
| 构造 | `LumePlugin::new(device: Arc<dyn lume_rhi::Device>)` | `LumelitePlugin::new(device: wgpu::Device, queue: wgpu::Queue)` 或 `new_with_config(...)` |
| 共同 trait | **RenderBackend**（render-api） | **RenderBackend**（render-api） |
| `prepare` | `fn prepare(&mut self, extracted: &ExtractedMeshes)` | 签名一致；已实现：Mesh → wgpu Buffer 并缓存 |
| `render_frame` | `fn render_frame(&mut self, view: &ExtractedView) -> Result<(), String>` | 签名一致；已实现：GBuffer + Light Pass，内部 submit |

**说明**：构造时 Lume 需要 `lume_rhi::Device`，Lumelite 需要 `wgpu::Device` + `wgpu::Queue`，因此宿主在「选择后端」时需根据配置创建对应 Plugin。创建之后，**调用方式完全一致**：`backend.prepare(&extracted)`、`backend.render_frame(&view)`。Lumelite 额外提供 `render_frame_to_swapchain(&view, &swapchain_view)` 用于窗口输出。

### 1.3 RHI 与 Renderer 层（不共用，仅概念对齐）

| 项目 | Lume | Lumelite |
|------|------|----------|
| RHI | **lume-rhi**（Vulkan，Device/Buffer/Texture/Pipeline 等） | **wgpu**（无 lume-rhi；wgpu 负责 Vulkan/Metal/DX12 等） |
| Renderer crate | **lume-renderer**（Render Graph、virtual_geom/gi 占位） | **lumelite-renderer**（GBuffer Pass、Light Pass、Present Pass；无 VG/GI） |
| 宿主是否直接依赖 | 宿主通过 LumePlugin 使用，可仅依赖 lume-bridge + render-api | 宿主通过 LumelitePlugin 使用，可仅依赖 lumelite-bridge + render-api |

**结论**：RHI 与 Renderer 的**实现**不共用；**接口兼容**体现在 render-api 类型 + RenderBackend 的 prepare/render_frame，宿主无需关心底层是 lume-rhi 还是 wgpu。

---

## 2. 行为差异（接口一致、实现为子集）

| 能力 | Lume | Lumelite |
|------|------|----------|
| Prepare 内容 | 计划：Mesh + 虚拟几何体资源 + GI 相关资源等；当前 prepare 为 TODO | 仅 Mesh → Vertex/Index Buffer 创建与缓存，已实现 |
| render_frame 内容 | 计划：Render Graph 含 VG 裁剪、GI 追踪、前向/延迟等；当前仅执行空图并 submit | Shadow → GBuffer → Light Pass（方向光/点光/聚光）→ Present；无 VG、无 GI |
| virtual_geom 模块 | 有占位（Cluster、VirtualGeometryManager 等） | lumelite-renderer 内为占位或空壳，不参与渲染 |
| gi 模块 | 有占位（GiSystem、GlobalSdf、SurfaceCache 等） | lumelite-renderer 内为占位或空壳，不参与渲染 |

这些差异**不**反映在 RenderBackend 的类型或方法签名上，仅反映在「传入了 VG/GI 相关数据时 Lumelite 会忽略」或「Lumelite 的 prepare/render_frame 内部不调用 VG/GI」。

---

## 3. 可选/扩展字段与未来兼容

- **ExtractedMesh**：若 Lume 未来增加字段（如 cluster_id、lod、gi_proxy 等），Lumelite 可保留该结构体与 Lume 一致，对未知字段忽略即可；这样宿主无需为 Lumelite 维护两套 Extract 类型。
- **RenderBackend**：若 Lume 增加 `prepare_gi`、`prepare_vg` 等可选方法（或 trait 扩展），Lumelite 可实现为空方法或 `Ok(())`，保持同一 trait 形态，便于上层统一调用。

---

## 4. 从 Lumelite 迁移到 Lume 的推荐步骤

1. **替换 Bridge 与 Renderer**  
   - 将宿主依赖从 `lumelite-bridge` 换为 `lume-bridge`，构造从 `LumelitePlugin::new(device, queue)` 改为 `LumePlugin::new(arc_dyn_device)`，device 来自 `lume_rhi::create_device(...)`。  
   - Lume 的 prepare 与 render_frame 需在 Lume 侧实现闭环（当前 Lume 的 prepare 为 TODO，render_frame 未使用 view/prepare 数据）。

2. **扩展 LumePlugin::prepare**  
   - 在 Lume 的 prepare 中增加对 Mesh 的上传（与 Lumelite 类似），以及对 VG 资源（Cluster、Indirect 等）和 GI 资源（SDF、Surface Cache 等）的上传与注册，调用 Lume 的 VirtualGeometryManager / GiSystem 等。

3. **扩展 render_frame**  
   - 在 Lume 的 render_frame 中根据 ExtractedView 与 Prepare 阶段注册的资源，构建 Render Graph（含 VG 裁剪、VG 绘制、GI 追踪与累积等），执行图并 submit + present。

4. **依赖与构建**  
   - 宿主 Cargo 依赖从 `lumelite-renderer`、`lumelite-bridge` 改为 `lume-rhi`、`lume-renderer`、`lume-bridge`（path 或 git）；**render-api 保持不变**，仍由根目录 render-api 提供类型与 RenderBackend 定义。

5. **数据与资源**  
   - 若 Lume 需要额外 Extract 数据（如 cluster 流送请求、GI 代理网格），在宿主引擎的 Extract 阶段补充写入 Extracted* 或 Lume 约定的扩展结构；Lumelite 时期可对这些字段留空或默认值。

---

## 5. 小结

- **接口兼容**：Extracted* 类型与 RenderBackend（prepare、render_frame）由 render-api 统一；Lume 与 Lumelite 各自实现，宿主可同一套调用逻辑切换后端。  
- **RHI/Renderer**：Lume 使用 lume-rhi + lume-renderer，Lumelite 使用 wgpu + lumelite-renderer，两者不共用实现，仅概念与高层 API 对齐。  
- **行为子集**：Lumelite 已实现 Mesh 上传与 GBuffer + Light + Present 完整闭环；Lume 为有良好基础、未完成闭环的后端。  
- **迁移路径**：替换为 lume-bridge/lume-renderer/lume-rhi、实现 Lume 的 prepare 与 render_frame 闭环、补充依赖与数据即可从 Lumelite 平滑迁移到 Lume。
