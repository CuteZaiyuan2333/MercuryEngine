# Lume 渲染引擎架构设计文档

## 1. 愿景与目标
Lume 是 **独立的渲染引擎**，作为 MercuryEngine 的高性能、高级渲染后端，旨在支持：
- **虚拟几何体 (Virtual Geometry)**：类似 UE5 Nanite 的 GPU 驱动裁剪与流送。
- **全局光照 (Global Illumination)**：类似 UE5 Lumen 的动态全局光照方案。
- **后端策略**：**直接基于 Vulkan 构建**，不依赖任何第三方渲染引擎或图形 API（如 WGPU、Bevy 等）；RHI 层预留扩展点，**计划中仅额外支持 Metal**。不计划支持 OpenGL、DirectX 或游戏主机平台。

## 2. 分层架构图

### 2.1 Lume RHI (Rendering Hardware Interface)
底层抽象层，**Vulkan 为首选与主实现**，接口设计预留 Metal 等后端扩展。
- **特性**：Bindless 资源管理、显式显存堆、计算着色器优先。
- **设计**：由 Lume 自主定义的资源与命令模型，不依赖 wgpu 或其他引擎的 API。

### 2.2 Lume Core (Rendering Pipeline)
核心算法层，实现高级特性，**仅依赖 Lume RHI**。
- **Virtual Geometry Manager**：管理 Cluster 划分、DAG 遍历、GPU 裁剪、软/硬光栅化切换。
- **Lumen-like GI System**：构建场景 SDF、光线追踪、时域累积降噪。
- **Render Graph (Frame Graph)**：管理任务依赖、资源生命周期和同步屏障。

### 2.3 Mercury Bridge (High-level API)
面向 MercuryEngine 的对接层，**不依赖 Bevy 或其它渲染引擎**。
- **Render App**：独立的渲染线程/子应用。
- **ECS Integration**：通过 Extract 阶段将主世界组件同步至渲染世界（由 MercuryEngine 与 Lume 约定）。
- **Plugins**：声明式的渲染功能扩展。

## 3. 开发流程规划

### 阶段一：Vulkan RHI 与同步机制 (Month 1-2)
1. 基于 **Vulkan** 实现 Lume RHI 的 `Buffer`, `Texture`, `Pipeline` 等抽象。
2. 构建 Vulkan 命令缓冲与队列提交。
3. 实现显式同步（Fence, Semaphore）。
4. RHI 接口设计时预留后端 trait，便于后续接入 Metal。

### 阶段二：Render Graph 与数据驱动 (Month 3)
1. 构建 Lume 自有 Render Graph 核心，支持自动屏障生成。
2. 实现 Extract/Prepare/Upload 数据流（与 MercuryEngine 约定，不依赖 Bevy）。

### 阶段三：虚拟几何体原型 (Month 4-6)
1. 离线端：Mesh 预处理（Cluster 划分）。
2. GPU 端：两级裁剪（Instance/Cluster）和 Indirect Draw。
3. 实现基于 Compute Shader 的软光栅（用于极小三角形）。

### 阶段四：全局光照 (Month 7-9)
1. 实现场景 SDF 生成。
2. 实现基于 Surface Cache 的光照追踪。
3. 实现动态漫反射与镜面反射。

## 4. 可行性评估结论
**可行性：高**。
Lume 作为独立引擎直接基于 Vulkan 开发，可完全掌控管线与数据流。UE5 的开源代码提供虚拟几何体与 GI 的算法参考。关键难点在于 **数据流转的高效性**（如何快速地从磁盘将 Cluster 流送至显存）以及 **RHI 抽象在 Vulkan/Metal 间的清晰分层**。
