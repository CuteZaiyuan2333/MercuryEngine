# Lumelite 项目目录结构

Lumelite 在**高层**与 Lume 保持 Bridge/数据流兼容，但 **RHI 使用 wgpu**、**光照采用 Deferred（GBuffer + Light Pass）**。以下为采用 wgpu + 延迟光照的目录结构及各目录职责说明。

## 1. Workspace 根目录

```text
lumelite/
├── Cargo.toml              # workspace，members: lumelite-renderer, lumelite-bridge
├── Cargo.lock
├── lumelite-renderer/      # 基于 wgpu：Render Graph 概念 + GBuffer Pass + Light Pass
├── lumelite-bridge/        # 与 Lume 接口一致（render-api 类型 + RenderBackend）；LumelitePlugin，内部用 wgpu
└── shaders/ 或 各 crate 内 embed  # WGSL：gbuffer.wgsl, lights.wgsl, present.wgsl（BRDF/光照合并在 lights.wgsl）
```

**示例程序**：已迁移至仓库根目录 **debug/**，见下方「debug/（示例与调试）」。

**说明**：RHI 不再使用 `lume-rhi`，改为在 renderer/bridge 中**直接依赖 `wgpu`**；着色器为 WGSL，光照与 BRDF 采用标准算法（Lambert、GGX/Schlick/Smith 等）。

## 2. RHI：直接使用 wgpu（无独立 lume-rhi crate）

- **依赖**：`wgpu` crate（及可选 `wgpu::Instance`、`raw_window_handle` 等）。
- **职责**：所有 Buffer、Texture、RenderPipeline、RenderPass、Queue 均由 wgpu 提供；不单独建一层「Lumelite RHI」抽象，除非后续需要多后端（当前仅 wgpu）。
- **着色器**：WGSL 源码可放在 `lumelite-renderer/shaders/` 或通过 `include_str!` 嵌入；管线创建时使用 `device.create_shader_module(wgpu::ShaderSource::Wgsl(...))`。

## 3. lumelite-renderer（渲染核心：wgpu + 延迟光照）

```text
lumelite-renderer/
├── Cargo.toml              # dependencies: wgpu, bytemuck 等
├── src/
│   ├── lib.rs              # Renderer, render_frame() 驱动 GBuffer → Light Pass
│   ├── graph/              # Render Graph 概念：Pass 依赖、资源句柄（基于 wgpu 资源）
│   │   └── mod.rs
│   ├── gbuffer/            # GBuffer Pass：多 RT + Depth
│   │   └── mod.rs
│   ├── light_pass/         # Light Pass：方向/点/聚/天光，加性混合
│   │   └── mod.rs
│   ├── present/            # Present Pass：Light Buffer → 交换链，色调映射
│   │   └── mod.rs
│   ├── resources/          # 纹理/缓冲创建与复用（GBuffer、Light Buffer、Depth、Shadow Map）
│   │   └── mod.rs
│   └── shadows/            # Shadow Map Pass：方向光深度渲染，单 cascade
│       └── mod.rs
├── shaders/                # WGSL
│   ├── gbuffer.wgsl        # GBuffer Pass：顶点/片段，多 RT 输出
│   ├── lights.wgsl         # Light Pass：BRDF、方向光/点光/聚光（Lambert、GGX、Schlick、Smith）
│   ├── present.wgsl        # Present Pass：Light Buffer 采样、色调映射（Reinhard/None）
│   └── shadow.wgsl         # Shadow Pass：深度输出，光空间渲染
```

### 3.1 无 virtual_geom / gi

- Lumelite 不实现虚拟几何体与全局光照；不保留对应模块，或仅保留空占位类型以便与 Lume 的命名空间对齐（可选）。

### 3.2 GBuffer、Light Pass 与 Shadow Pass

- **GBuffer**：多 RT（Color+AO、Normal、Roughness/Metalness/Specular、CustomData）+ Depth；材质输出 Color、Normal、Roughness、Metalness、Specular、AO、ShadingModel、可选 CustomData。
- **Light Pass**：方向光全屏；点光、聚光全屏 + 距离/锥体衰减；加性混合；BRDF 为 Lambert + GGX/Schlick/Smith；ExtractedView.point_lights、spot_lights。
- **Shadow Pass**：方向光视角深度渲染至 shadow map；`LumeliteConfig::shadow_enabled`、`shadow_resolution`；单 cascade。

## 4. lumelite-bridge（对接层，与 Lume 接口一致）

```text
lumelite-bridge/
├── Cargo.toml              # dependencies: render-api, lumelite-renderer, wgpu
├── src/
│   ├── lib.rs              # 导出 LumelitePlugin（不定义 Extract 类型，使用 render-api）
│   └── plugin.rs           # LumelitePlugin::new(device, queue) / new_with_config, prepare, render_frame, render_frame_to_swapchain
```

- **职责**：实现 **render_api::RenderBackend**，与 Lume 的 **类型与 API 一致**（Extract 类型来自 render-api）；内部使用 wgpu 与 lumelite-renderer（GBuffer + Light Pass）。
- **prepare**：遍历 `ExtractedMeshes`，创建或复用 wgpu Buffer（顶点/索引），写入 mesh_cache 供 render_frame 使用。
- **render_frame**：根据 `ExtractedView` 与 mesh_cache 构建 MeshDraw，执行 GBuffer Pass → Light Pass，内部 submit。**render_frame_to_swapchain**：同上并 Present 到给定 swapchain view。

## 5. debug/（示例与调试）

示例程序已从 lumelite 中移出，位于仓库根目录 **debug/**：

```text
debug/
├── Cargo.toml              # dependencies: lumelite-renderer, lumelite-bridge, render-api (path), wgpu, winit 等
└── src/
    └── bin/
        ├── minimal_wgpu.rs           # 无窗口，最小 wgpu 初始化
        ├── gbuffer_light_window.rs   # 窗口 + GBuffer + Light Pass（LumelitePlugin + swapchain）
        └── plugin_loop.rs            # RenderBackend + LumelitePlugin + ExtractedMeshes 离屏闭环
```

- 运行示例：在**仓库根目录**执行 `cargo run -p debug --bin <minimal_wgpu|plugin_loop|gbuffer_light_window>`；或在 **debug** 目录下执行 `cargo run --bin <minimal_wgpu|plugin_loop|gbuffer_light_window>`。
- 示例使用 wgpu + WGSL，不再依赖 Vulkan 或 lume-rhi。

## 6. lume-tools（可选）

- **Lumelite**：不实现 Cluster/SDF 等；可不包含 lume-tools，或仅保留空壳以便与 Lume 工程结构一致。

## 7. 下一步行动建议

1. ~~**引入 wgpu**~~：已完成。lumelite-renderer / lumelite-bridge 已使用 wgpu。
2. ~~**光照管线**~~：已完成。GBuffer Pass + Light Pass（方向光/点光/聚光）+ Shadow Pass + Present。
3. ~~**实现 Bridge prepare**~~：已完成。ExtractedMeshes → wgpu Buffer，mesh_cache 复用。
4. ~~**打通一帧**~~：已完成。Extract → Prepare → render_frame（Shadow → GBuffer → Light → Present）→ submit + present。
5. **可选**：Light Pass 采样 shadow map 实现 PCF 软阴影；天光实现；model buffer 复用优化。
