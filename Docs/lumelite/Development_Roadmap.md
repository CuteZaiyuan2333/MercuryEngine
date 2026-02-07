# Lumelite 详细开发路线图

本文档给出 Lumelite 的**分阶段、可验收**的开发计划，与 Lume 架构对齐且不包含虚拟几何体与全局光照。时间估算以「人周」为单位，可按实际情况调整。

---

## 当前实现状态（与路线图对照）

**说明**：路线图最初按「与 Lume 同源 RHI/Renderer 再裁剪」的思路编写。实际实现采用了**独立技术选型**：Lumelite 自始使用 **wgpu** 与 **lumelite-renderer**（GBuffer + Light + Present），不包含 lume-rhi/lume-renderer 源码。

**当前已完成**：

- **阶段〇**：lumelite 独立 workspace，可 `cargo build`；示例为 `minimal_wgpu`、`plugin_loop`、`gbuffer_light_window`（无 minimal_vulkan/ubo_triangle_window，因不使用 lume-rhi）。
- **阶段一**：RHI 为 wgpu，Render Graph 概念在 lumelite-renderer 中以 Pass 依赖与 FrameResources 体现；无「与 Lume 一致的 lume-rhi」。
- **阶段二**：virtual_geom/gi 在 lumelite-renderer 中为占位或空壳，不参与渲染。
- **阶段三～四**：LumelitePlugin 已实现 prepare（ExtractedMeshes → wgpu Buffer + mesh_cache）与 render_frame 闭环（GBuffer + Light Pass，submit）；并支持 `render_frame_to_swapchain` 做窗口输出。
- **阶段五**：接口兼容文档已更新（Interface_Compatibility、Backend_Switch、Current_Status）；RenderBackend 与 render-api 类型与 Lume 对齐。

后续可按「可选扩展」（多光源、resize、阴影、示例独立化等）推进，详见 [Current_Status.md](Current_Status.md)。

---

## 阶段〇：现状确认与代码基线（第 0 周）

### 目标

- 确认 lumelite 目录可独立构建、运行示例，与 Lume 在接口上对齐（render-api、RenderBackend）。

### 任务

| 序号 | 任务 | 验收标准 | 状态 |
|------|------|----------|------|
| 0.1 | 在 lumelite 根目录执行 `cargo build`，所有 crate 通过编译 | 无编译错误 | ✅ |
| 0.2 | 在仓库根目录运行 `cargo run -p debug --bin minimal_wgpu` | 无 panic，正常退出 | ✅ |
| 0.3 | 在仓库根目录运行 `cargo run -p debug --bin gbuffer_light_window` | 窗口出现，三角形可见 | ✅ |
| 0.4 | 阅读 Lume 文档并记录与 Lumelite 的差异清单 | 差异清单写入 Docs/lumelite | ✅ |

### 产出

- 可构建、可运行的 lumelite 代码基线。
- 与 Lume 的差异清单（用于后续接口兼容文档）。

---

## 阶段一：RHI 与 Render Graph 稳定性（第 1–2 周）

**当前状态**：Lumelite 采用 **wgpu** 作为 RHI，未使用 lume-rhi；Render Graph 概念在 lumelite-renderer 中以 Pass 与 FrameResources 实现。本节保留原路线描述供参考。

### 目标

- 确认 Lumelite 使用的 RHI 稳定、Render 管线可执行。

### 任务

| 序号 | 任务 | 验收标准 | 状态 |
|------|------|----------|------|
| 1.1 | （原为 lume-rhi 对照）使用 wgpu 作为唯一 RHI | wgpu Device/Queue/Buffer/Texture/RenderPipeline 等满足 GBuffer + Light 需求 | ✅ |
| 1.2 | Pass 依赖与资源生命周期（FrameResources） | GBuffer Pass → Light Pass → Present 顺序正确 | ✅ |
| 1.3 | 示例：minimal_wgpu、gbuffer_light_window | 帧中能观察到预期绘制结果 | ✅ |
| 1.4 | （可选）wgpu 验证 | 无驱动/验证报错 | 可选 |

### 产出

- 稳定的 wgpu + Pass 管线使用方式。
- 基于 LumelitePlugin 的窗口示例（gbuffer_light_window）。

---

## 阶段二：virtual_geom 与 gi 的 Lumelite 策略（第 2 周）

### 目标

- 明确 Lumelite 中 virtual_geom 与 gi 不实现，仅保留空壳（可选）或占位。
- 保证对外 API 为 RenderBackend（render-api），与 Lume 接口对齐。

### 任务

| 序号 | 任务 | 验收标准 | 状态 |
|------|------|----------|------|
| 2.1 | virtual_geom / gi 在 lumelite-renderer 中为占位或空壳，不参与渲染 | 编译通过；渲染路径不依赖 VG/GI | ✅ |
| 2.2 | 文档中说明与 Lume 的差异及迁移方式 | Interface_Compatibility、Current_Status 已写明 | ✅ |
| 2.3 | lumelite-renderer 不对外承诺 VG/GI API | 仅 GBuffer + Light + Present 为稳定能力 | ✅ |

### 产出

- 确定的模块策略（空壳/占位，不实现）。
- 文档中写明与 Lume 的差异及日后迁移方式。

---

## 阶段三：Bridge Prepare 与 Mesh 上传（第 3–4 周）

### 目标

- 实现 LumelitePlugin::prepare：将 ExtractedMeshes 转为 wgpu Buffer（顶点/索引），并供 render_frame 使用。

### 任务

| 序号 | 任务 | 验收标准 | 状态 |
|------|------|----------|------|
| 3.1 | 内部「Mesh 资源」表示（CachedMesh：vertex_buf, index_buf, index_count, transform） | 可被 render_frame 构建为 MeshDraw | ✅ |
| 3.2 | LumelitePlugin 中维护 mesh_cache（entity_id → CachedMesh） | 相同 entity 且顶点/索引长度未变时复用已有 Buffer，仅 write_buffer 更新；entity 消失则移除缓存；长度变化则重建 Buffer | ✅ |
| 3.3 | prepare：遍历 ExtractedMeshes，创建 wgpu Buffer 并写入顶点/索引，插入 mesh_cache | plugin_loop / gbuffer_light_window 可验证 | ✅ |
| 3.4 | render_frame 从 mesh_cache 构建 MeshDraw 列表，传入 Renderer::encode_frame | 帧中正确绘制 Mesh | ✅ |
| 3.5 | 顶点格式：与 render-api 约定一致（如 position + normal，6 f32/顶点）；在示例与文档中体现 | 见 plugin_loop、Current_Status | ✅ |

### 产出

- LumelitePlugin::prepare 完整实现：ExtractedMeshes → wgpu Buffer + mesh_cache。
- render_frame 使用 prepare 数据完成 GBuffer + Light 绘制。

---

## 阶段四：单 Pass 前向渲染与 render_frame 闭环（第 4–5 周）

### 目标

- render_frame 使用 Prepare 阶段数据，完成 GBuffer + Light Pass（及可选 Present 到 Swapchain）。
- 实现从 Extract → Prepare → render_frame（或 render_frame_to_swapchain）→ submit（+ present）的完整一帧。

### 任务

| 序号 | 任务 | 验收标准 | 状态 |
|------|------|----------|------|
| 4.1 | 当前帧渲染目标：FrameResources（GBuffer、Light Buffer、Depth）；Swapchain 由宿主传入 swapchain_view | gbuffer_light_window 每帧传入 swapchain_view | ✅ |
| 4.2 | GBuffer Pass：绘制 Mesh 到 GBuffer + Depth；Light Pass：方向光全屏 | encode_frame 含 GBuffer + Light | ✅ |
| 4.3 | render_frame 使用 ExtractedView（view_proj、viewport_size）与 mesh_cache 构建 MeshDraw，调用 encode_frame | 离屏与窗口示例均可用 | ✅ |
| 4.4 | 示例：plugin_loop（离屏）、gbuffer_light_window（prepare + render_frame_to_swapchain） | 窗口内能看到 Mesh 渲染结果 | ✅ |
| 4.5 | 处理 resize：FrameResources 与 Swapchain 在窗口缩放时的重建 | 当前由 ensure_frame_resources 按 width/height 更新；swapchain 由宿主重建后传入 | 部分（可后续完善） |

### 产出

- 从 Prepare → render_frame（/ render_frame_to_swapchain）→ submit → present 的完整闭环。
- 使用 LumelitePlugin + ExtractedMeshes 的示例：plugin_loop、gbuffer_light_window。

---

## 阶段五：接口兼容性与文档收口（第 5–6 周）

### 目标

- 与 Lume 在 render-api 与 RenderBackend 上保持兼容：类型一致、prepare/render_frame 签名一致。
- 完成 Docs/lumelite 下的架构、结构、路线图、接口兼容说明的更新。

### 任务

| 序号 | 任务 | 验收标准 | 状态 |
|------|------|----------|------|
| 5.1 | 对照 render-api 的 Extracted*、RenderBackend；LumelitePlugin 实现 prepare/render_frame | 类型与 trait 一致；Lumelite 未使用字段可忽略 | ✅ |
| 5.2 | 宿主仅依赖 render-api + lumelite-bridge（或 lume-bridge）即可驱动 prepare + render_frame | 无 Lume 专有扩展依赖 | ✅ |
| 5.3 | Interface_Compatibility.md：兼容点、差异点、迁移到 Lume 的步骤 | 已写明 wgpu/LumelitePlugin 与 Lume 的差异 | ✅ |
| 5.4 | Development_Roadmap.md、Current_Status.md 与实现状态一致 | 路线图可追溯，当前状态文档正确 | ✅ |

### 产出

- 接口兼容性文档与路线图已更新。
- 「从 Lumelite 迁移到 Lume」说明已写入 Interface_Compatibility.md。

---

## 阶段六（可选）：扩展与优化

- **多 Mesh / 多 Pass**：多个不透明物体、简单排序（如按材质）。
- **简单相机**：从 ExtractedView 解析 view_proj/viewport，支持多相机（仅主视口）。
- **基础阴影**：可选的 Shadow Map Pass（单光源、单 cascade），仍不涉及 VG/GI。
- **性能**：Staging Buffer 复用、Descriptor 池复用、减少每帧分配。

---

## 依赖关系小结

```
阶段〇 → 阶段一 → 阶段二
              ↓
        阶段三 → 阶段四 → 阶段五
              ↑__________|
```

- 阶段一、二可部分并行（如先定 RHI/Graph，再定 VG/GI 策略）。
- 阶段三与四需顺序执行（先 Prepare 再闭环）。
- 阶段五依赖阶段四的闭环完成。

---

## 验收检查表（整体）

- [x] lumelite 在无 Lume 源码的情况下可独立构建与运行示例。
- [x] LumelitePlugin::prepare 能正确将 ExtractedMeshes 转为 wgpu Buffer 并缓存（mesh_cache）。
- [x] LumelitePlugin::render_frame / render_frame_to_swapchain 能正确执行 GBuffer + Light（+ Present）并 submit。
- [x] 与 Lume 的 Extracted* 与 RenderBackend 接口一致（通过 render-api）；文档中已说明差异与迁移方式。
