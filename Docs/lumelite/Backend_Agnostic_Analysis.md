# 后端无关渲染：需求分析与实现路径

本文档审视当前代码，分析「项目只调用 render-api、后端无关地渲染，宿主不直接调用 wgpu」这一需求是否可达，以及需要做哪些改动。

---

## 1. 当前状态：宿主在哪里接触了后端 API？

### 1.1 render-api 本身（已后端无关）

- **类型**：`ExtractedMesh`、`ExtractedMeshes`、`ExtractedView` 均为纯数据，无 wgpu/Vulkan 类型。
- **Trait**：`RenderBackend` 仅有 `prepare(&ExtractedMeshes)` 与 `render_frame(&ExtractedView) -> Result<(), String>`，接口层面无后端类型。

因此 **render-api 的接口定义** 已经满足「后端无关」。

### 1.2 宿主当前对后端的直接依赖

| 环节 | 当前做法 | 是否后端无关 |
|------|----------|--------------|
| **构造后端** | 宿主自己调用 `wgpu::Instance::default()`、`request_adapter`、`request_device`，得到 `(Device, Queue)` 后调用 `LumelitePlugin::new(device, queue)` | 否：宿主直接使用 wgpu API |
| **离屏一帧** | `backend.prepare(extracted); backend.render_frame(&view)` | 是：仅用 render-api |
| **窗口输出** | 宿主创建 `wgpu::Surface`、`surface.configure(device, ...)`、`get_current_texture()`、`TextureView`，再调用 `plugin.render_frame_to_swapchain(&view, &wgpu::TextureView)`；还需保存 `wgpu::Instance` 以保持 Device 有效 | 否：宿主大量使用 wgpu（Instance/Surface/TextureView/configure） |
| **LumelitePlugin 暴露** | `device()`、`queue()`、`renderer()`、`render_frame_to_swapchain(view, &wgpu::TextureView)` | 否：对外暴露 wgpu 类型与 swapchain 细节 |

结论：**离屏渲染**（如 `plugin_loop`）在「只依赖 render-api 调用」的意义上已经后端无关；**构造**与**窗口呈现**两处宿主仍直接依赖 wgpu，需求尚未完全达到。

---

## 2. 需求是否可以达到？

**可以。** 做法是：把「创建后端」和「渲染到窗口」都收进 render-api 的抽象里，由后端在内部完成 wgpu/Vulkan 的创建与 swapchain 操作，宿主只看到 render-api 与窗口句柄。

---

## 3. 实现路径（概要）

### 3.1 构造阶段：后端由「窗口/句柄」创建，不暴露 Device/Queue

- **思路**：宿主不创建 wgpu Instance/Device/Queue，而是提供「窗口」或「显示句柄」，由「工厂」或「后端构造函数」在内部完成 wgpu 初始化并返回 `Box<dyn RenderBackend>`（或枚举）。
- **render-api 侧**：可增加「后端工厂」抽象（例如在 feature 或独立 crate 中），或约定「由各 bridge 提供 `from_window` 之类构造函数，返回 `Box<dyn RenderBackend>`」。宿主只依赖 render-api + 一个选中的 bridge，调用该构造函数，不直接依赖 wgpu。
- **lumelite-bridge**：提供例如 `LumelitePlugin::from_window(window: &impl HasRawWindowHandle) -> Result<Box<dyn RenderBackend>, String>`，内部完成 Instance/Adapter/Device/Queue/Surface 的创建与保存。

这样宿主侧不再出现 `wgpu::Instance`、`request_adapter`、`request_device`、`LumelitePlugin::new(device, queue)`。

### 3.2 窗口呈现：由 RenderBackend 统一「渲染并 Present」

- **问题**：当前 `render_frame_to_swapchain(view, &wgpu::TextureView)` 要求宿主每帧拿 wgpu 的 TextureView，因此宿主必须做 Surface/configure/get_current_texture。
- **思路**：在 render-api 中扩展「可呈现到窗口」的能力，例如增加 trait 方法或扩展 trait：
  - 例如：`fn render_frame_to_window(&mut self, view: &ExtractedView, window: &impl HasRawWindowHandle) -> Result<(), String>`
  - 或：`RenderBackendWindow` 子 trait，包含 `render_frame_and_present(&mut self, view, window)`。
- **后端职责**：在实现内部持有/创建 Surface，每帧在 `render_frame_to_window` 内完成 get_current_texture、encode、present，宿主不再接触 Surface/TextureView/Instance。

这样宿主每帧只需：`backend.prepare(extracted); backend.render_frame_to_window(&view, &window)?`（或类似），无需任何 wgpu 类型。

### 3.3 可选：Resize 与配置

- 若需要处理 resize，可由 `render_frame_to_window` 内部根据当前窗口尺寸重新 configure surface，或由 render-api 定义 `notify_resize(&mut self, width, height)`，由后端在下一帧使用新尺寸。
- 若需要 swapchain 格式等配置，可在「从窗口创建后端」时通过 render-api 的配置结构体传入（例如可选 `WindowBackendConfig`），而不暴露 wgpu 的 TextureFormat 等。

---

## 4. 小结

| 需求 | 当前 | 可达性 | 关键改动 |
|------|------|--------|----------|
| 宿主只依赖 render-api 做「渲染调用」 | 仅离屏满足 | 是 | 已满足（离屏）；窗口需见下两行 |
| 宿主不创建 / 不持有 wgpu Device/Queue/Instance | 不满足 | 是 | 后端由「窗口」构造并返回 `dyn RenderBackend` |
| 宿主不直接调用 wgpu（含 Surface/TextureView） | 不满足 | 是 | render-api 增加「按窗口呈现」接口，后端内部完成 swapchain/present |

实现上述两点（构造抽象 + 窗口呈现抽象）后，宿主代码可以做到：**只依赖 render-api 与窗口类型（如 winit + HasRawWindowHandle），不直接调用任何 wgpu API**，从而在「调用 render-api、后端无关地渲染」和「至少不直接调用 wgpu」两个层面上都满足需求。

---

## 5. 已实现（当前代码）

- **render-api**：新增 `RenderBackendWindow` trait（`render_frame_to_window(view, raw_window_handle, raw_display_handle)`），并 re-export `RawWindowHandle` / `RawDisplayHandle`。
- **lumelite-bridge**：新增 `LumeliteWindowBackend` 与 `LumeliteWindowBackend::from_window(window)`，返回 `Box<dyn RenderBackendWindow>`；内部使用 wgpu 的 `create_surface_unsafe` 与 swapchain，宿主不接触 wgpu。
- **debug/gbuffer_light_window**：仅依赖 `render_api`、`lumelite_bridge::LumeliteWindowBackend`、`raw_window_handle`、`winit`；无 wgpu 或 lumelite_renderer 的直接调用。每帧调用 `backend.prepare(&extracted)` 与 `backend.render_frame_to_window(&view, raw_window, raw_display)`。
