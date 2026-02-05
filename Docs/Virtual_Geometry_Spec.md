# Lume 虚拟几何体 (Virtual Geometry) 技术方案

## 1. 核心思想
虚拟几何体通过将静态网格划分为成千上万个小的 **Cluster** (通常 128 个三角形)，并结合 **GPU 驱动的剔除 (GPU Driven Culling)**，实现像素级的几何细节而不受传统 Draw Call 和三角形数量的限制。实现**仅依赖 Lume RHI**（Vulkan / 计划中的 Metal），不依赖任何其它渲染引擎或图形 API。

## 2. 数据处理流水线 (Preprocessing)
1. **Cluster 划分**：使用 METIS 等图划分算法将网格切割。
2. **DAG 构建**：为不同的 LOD 级别构建有向无环图，确保 LOD 切换时无缝。
3. **BVH 加速空间索引**：用于快速裁剪。

## 3. 运行时渲染流程 (Runtime)
1. **Instance Culling**：在计算着色器中对整个物体进行裁剪（Frustum/Occlusion）。
2. **Cluster Culling**：针对通过初步裁剪的物体，进行更细致的 Cluster 裁剪。
3. **LOD Selection**：根据屏幕覆盖率自动选择合适的 DAG 节点。
4. **Drawing**：
   - **Hard Rasterization**：对于较大的 Cluster，使用传统的管线（支持 Mesh Shader）。
   - **Soft Rasterization**：对于小于 1 像素的 Cluster，使用 Compute Shader 手写光栅化以避免原子操作争用。

## 4. 显存流送 (Streaming)
- **LRU Cache**：在显存中维护一个 Cluster 池。
- **Request Queue**：当 GPU 发现某个 Cluster 需要显示但不在显存时，向 CPU 发送请求。
- **Async Copy**：使用独立拷贝队列（Transfer Queue）加载数据。

## 5. 待解决难点
- **UV/材质切换**：如何在单个 Draw Call 中处理多种材质（Bindless Texture 方案）。
- **阴影处理**：与 Virtual Shadow Maps (VSM) 的集成。
