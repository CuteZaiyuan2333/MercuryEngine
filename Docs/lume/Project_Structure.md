# Lume 项目目录结构建议

Lume 为 **独立渲染引擎**，直接基于 Vulkan，不依赖 WGPU、Bevy 或其他渲染引擎。RHI 预留后端扩展，计划中仅额外支持 Metal。

建议采用如下结构：

```text
lume/
├── lume-rhi/             # 底层硬件抽象 (Vulkan 主实现，接口预留 Metal)
│   ├── src/
│   │   ├── vulkan/       # Vulkan 实现（主后端）
│   │   ├── metal/        # Metal 实现（计划支持）
│   │   └── lib.rs        # 统一接口定义 (Device, Buffer, Texture, Pipeline, CommandEncoder)
├── lume-renderer/        # 核心渲染逻辑 (仅依赖 lume-rhi，实现 VG & GI)
│   ├── src/
│   │   ├── virtual_geom/ # 虚拟几何体逻辑
│   │   ├── gi/           # 全局光照逻辑
│   │   ├── graph/        # Render Graph 引擎
│   │   └── lib.rs
├── lume-bridge/          # MercuryEngine 对接层（无 Bevy/WGPU 依赖）
│   ├── src/
│   │   ├── plugin.rs     # Lume 渲染插件
│   │   ├── extract.rs    # 从主世界到渲染世界的数据提取
│   │   └── lib.rs
└── lume-tools/           # 离线工具 (Mesh 预处理、SDF 生成等)
```

## 下一步行动建议
1. **Vulkan 后端**：在 `lume-rhi` 中实现 Vulkan 版 `Device`/Buffer/Texture/Pipeline/CommandEncoder，并实现 Fence/Semaphore。
2. **RHI Trait 设计**：在 `lib.rs` 中稳定后端无关 trait，为后续 Metal 预留实现位。
3. **原型验证**：用 Lume RHI（Vulkan）运行一个最小计算/绘制管线，确保无任何第三方渲染依赖。
