# Lume 全局光照 (Global Illumination) 技术方案

## 1. 技术选型：类 Lumen 方案
Lume GI 旨在实现在不需要预计算光照贴图的情况下，提供实时的、可交互的全局光照。实现**仅依赖 Lume RHI**（Vulkan / 计划中的 Metal），不依赖任何其它渲染引擎或图形 API。

## 2. 核心组件

### 2.1 场景描述：Mesh SDF (Signed Distance Fields)
- 为每个静态物体生成低分辨率的 SDF。
- 在运行时将多个 Mesh SDF 组合成 **Global SDF**。
- 用途：快速光线步进 (Ray Marching) 以模拟漫反射阴影和光线追踪。

### 2.2 Surface Cache
- 将物体的表面属性（BaseColor, Normal, Emissive）缓存进 Atlas。
- 用途：光线击中物体时，直接查找缓存以获取光照，避免重新运行复杂的着色器。

### 2.3 光线追踪逻辑
- **Short Range (Screenspace)**：使用屏幕空间追踪处理近处细节。
- **Mid-Long Range (SDF)**：使用 SDF 步进处理远处光照。

### 2.4 时域降噪 (Temporal Accumulation)
- 每一帧只追踪少量光线（如 1 spp），利用时域信息（Motion Vectors）进行累积和降噪。

## 3. 性能目标
- 在 RTX 3060 级别显卡上，1080p 开启 GI 目标帧率 60fps。
- 支持动态光源（如太阳转动）的实时更新。

## 4. 与虚拟几何体的集成
- GI 追踪需要一个低模代理。虚拟几何体的 DAG 结构天然提供了不同精度的几何表示，可以作为光线追踪的加速结构。
