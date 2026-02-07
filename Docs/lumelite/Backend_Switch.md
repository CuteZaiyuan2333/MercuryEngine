# 与 Lume 切换：统一 Render API 与宿主用法

本文档说明如何在同一程序中通过**同一套类型与接口**在 Lume 与 Lumelite 之间切换渲染后端，无需重复写两套 Extract/Prepare/Render 逻辑。

## 1. 架构概览

```
┌──────────────────────────────────────────────────────────────────┐
│  宿主（MercuryEngine 或示例）                                      │
│  只依赖 render-api；每帧填充 ExtractedMeshes / ExtractedView，     │
│  调用 backend.prepare(&extracted); backend.render_frame(&view)      │
└──────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    ▼                               ▼
┌─────────────────────────────┐   ┌─────────────────────────────┐
│  Lume 实现                    │   │  Lumelite 实现                │
│  lume-bridge::LumePlugin     │   │  lumelite-bridge::LumelitePlugin │
│  实现 RenderBackend          │   │  实现 RenderBackend          │
│  (Arc<dyn Device>)           │   │  (wgpu::Device, wgpu::Queue)│
└─────────────────────────────┘   └─────────────────────────────┘
                    │                               │
                    ▼                               ▼
            lume-rhi (Vulkan)                 wgpu
```

- **render-api**（仓库根目录 `render-api/`）：定义并导出 `ExtractedMesh`、`ExtractedMeshes`、`ExtractedView` 和 `RenderBackend` trait，供宿主与任意后端共用。
- **Lume**：`lume/lume-bridge` 依赖 render-api，使用其类型并为 `LumePlugin` 实现 `RenderBackend`；`render_frame` 内部完成 submit，返回 `Result<(), String>`。
- **Lumelite**：`lumelite/lumelite-bridge` 依赖 render-api，使用其类型并为 `LumelitePlugin` 实现 `RenderBackend`；支持 `render_frame`（离屏）与 `render_frame_to_swapchain`（窗口输出）。

## 2. 统一类型（render-api）

宿主仅依赖 **render-api**，使用以下类型：

| 类型 | 说明 |
|------|------|
| `ExtractedMesh` | entity_id, vertex_data, index_data, transform, visible |
| `ExtractedMeshes` | meshes: HashMap<u64, ExtractedMesh> |
| `ExtractedView` | view_proj, viewport_size, directional_light（可选）, point_lights, spot_lights, sky_light |
| `RenderBackend` | trait: `prepare(&mut self, &ExtractedMeshes)`, `render_frame(&mut self, &ExtractedView) -> Result<(), String>` |

## 3. 宿主代码形态

### 3.1 统一调用（与后端无关）

```rust
use render_api::{ExtractedMeshes, ExtractedView, RenderBackend};

// backend 为 Box<dyn RenderBackend> 或 enum，在启动时由配置决定
fn run_frame(backend: &mut dyn RenderBackend, extracted: &ExtractedMeshes, view: &ExtractedView) -> Result<(), String> {
    backend.prepare(extracted);
    backend.render_frame(view)
}
```

### 3.2 构造时的差异

- **Lume**：`LumePlugin::new(device: Arc<dyn lume_rhi::Device>)`，device 来自 `lume_rhi::create_device(...)`。
- **Lumelite**：`LumelitePlugin::new(device: wgpu::Device, queue: wgpu::Queue)` 或 `new_with_config(device, queue, LumeliteConfig { ... })`，来自 wgpu 的 `request_device`。

创建之后，`prepare` / `render_frame` 的调用方式完全一致。

### 3.3 Lumelite 窗口输出（推荐：后端无关）

若希望**宿主不直接调用 wgpu**，使用 `LumeliteWindowBackend::from_window(window)` 得到 `Box<dyn RenderBackendWindow>`，每帧：

```rust
backend.prepare(&extracted);
backend.render_frame_to_window(&view, raw_window_handle, raw_display_handle)?;
```

其中 `raw_window_handle` / `raw_display_handle` 从窗口的 `window_handle()` / `display_handle()` 取得（见 `render_api::RawWindowHandle` / `RawDisplayHandle`）。示例：`debug/gbuffer_light_window`。

若需自行管理 swapchain，可继续使用 `LumelitePlugin::new(device, queue)` 与 `plugin.render_frame_to_swapchain(&view, &swapchain_view)`（宿主会依赖 wgpu）。

## 4. 示例

- **plugin_loop**（`debug`）：仅用 render-api 与 LumelitePlugin，演示 `prepare` + `render_frame` 一帧（离屏）。运行：在仓库根目录 `cargo run -p debug --bin plugin_loop`，或 `cd debug && cargo run --bin plugin_loop`。
- **gbuffer_light_window**（`debug`）：使用 LumeliteWindowBackend（后端无关），窗口 + 每帧 `prepare` + `render_frame_to_window`；Surface 缓存复用，处理 SurfaceError::Outdated/Lost。运行：在仓库根目录 `cargo run -p debug --bin gbuffer_light_window`，或 `cd debug && cargo run --bin gbuffer_light_window`。

宿主若希望“编译期或运行时选择 Lume 或 Lumelite”，可依赖 render-api 与可选 feature（如 `lume` / `lumelite`），根据配置构造对应 Plugin 并持有一个 `Box<dyn RenderBackend>` 或枚举，每帧调用同一套 `prepare` / `render_frame` 即可。

## 5. 小结

- **同一程序内简单切换**：通过根目录共享 **render-api**（统一类型 + `RenderBackend`），Lume 与 Lumelite 均实现该 trait，宿主只持有一个后端实例并调用同一套 prepare/render_frame。
- **lumelite 文件夹内不保存 Lume 代码**：lumelite 下仅有 lumelite-renderer、lumelite-bridge；示例位于仓库根目录 **debug/**，不包含任何 lume-* 源码。
