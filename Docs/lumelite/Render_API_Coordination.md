# 围绕单一 Render API 的 Lume / Lumelite 协调与可切换架构

本文档分析如何让 **Lume**（未来：纯虚拟几何体 + 类 Lumen 光照）与 **Lumelite**（类 Flax 延迟渲染）在**同一套 render-api** 下良好共存、可切换运行，供 MercuryEngine 在「轻量/高质量」与「超大规模/GI」两种模式间选择。

---

## 1. 设计原则

- **单一契约**：宿主只依赖 **render-api**，通过同一套类型（ExtractedMesh、ExtractedView 等）和同一套调用（prepare、render_frame）驱动渲染。
- **后端对等**：Lume 与 Lumelite 均实现 `RenderBackend`，**构造时**由宿主根据配置或能力选择其一，**运行时**调用方式一致。
- **能力子集**：render-api 描述的是「宿主能提供什么」；各后端只消费自己关心的部分，对超集字段可忽略或做最小解释，避免为迁就某一方而污染 API。
- **可选扩展**：新增字段或类型时，尽量采用「可选」或「默认可忽略」设计，使旧后端无需改动即可继续工作。

---

## 2. 能力矩阵与语义分工

| 能力 | Lumelite | Lume（规划） |
|------|----------|--------------|
| **几何** | 经典 Mesh（顶点/索引 Buffer，每帧 prepare 上传） | 虚拟几何体（Cluster 流送、GPU 裁剪、Indirect Draw）+ 可选经典 Mesh 回退 |
| **光照** | 延迟：GBuffer + 多光源 Light Pass（方向/点/聚/天光），Lambert + GGX/Schlick/Smith | 类 Lumen：SDF、光线追踪、Surface Cache、时域降噪 |
| **RHI** | wgpu（Vulkan/Metal/DX12 由 wgpu 选） | lume-rhi（Vulkan 为主，预留 Metal） |
| **Render Graph** | 概念一致，固定顺序 Shadow → GBuffer → Light → Present | 完整 Frame Graph，任务依赖与屏障 |
| **材质/贴图** | 当前：顶点色/常量；扩展：PBR 贴图（BaseColor/Normal/MR/AO） | 自有材质与 VG/GI 资源绑定 |

**协调要点**：

- **ExtractedMesh**：对 Lumelite 表示「要画的传统 Mesh」；对 Lume 可表示「传统回退 Mesh」或「VG 的 LOD/Proxy 占位」。若未来 Lume 引入「Cluster 请求」，可在 render-api 中增加可选 `cluster_request` 等，Lumelite 忽略即可。
- **ExtractedView**：两者共用（view_proj、viewport、光源列表）。Lumelite 用光源做延迟光照；Lume 用同一批光源参与 GI 或作为 fallback。
- **prepare**：Lumelite 将 mesh 转为 wgpu Buffer 并缓存；Lume 将 mesh 转为 RHI 资源、或登记 VG Cluster、或上传 GI 相关数据。**同一接口，不同实现**。
- **render_frame**：Lumelite 执行 GBuffer → Light → Present；Lume 执行 Render Graph（VG 绘制 + GI 追踪等）。**同一接口，不同实现**。

这样，宿主无需写两套逻辑，只需在**启动时**选择后端，并保证每帧填充的 Extract 数据在「语义上」满足当前后端即可（例如选 Lumelite 时可不填 cluster 相关；选 Lume 时可不填 PBR 贴图句柄）。

---

## 3. 可选扩展与兼容策略

为使两种风格长期兼容，建议：

### 3.1 类型扩展方式

- **ExtractedMesh**：  
  - 现有字段（entity_id, vertex_data, index_data, transform, visible）保持不变，两者共用。  
  - 新增字段一律 **Option** 或「带默认值」：例如 `material: Option<ExtractedMaterial>`、`vertex_format: VertexFormat`（默认 PositionNormal）等。  
  - Lumelite：使用 material 做 PBR 贴图绑定；使用 vertex_format 选择顶点布局。  
  - Lume：可忽略 material，或映射为自有材质 ID；vertex_format 在传统 Mesh 回退时使用。

- **ExtractedView**：  
  - 已具备 directional_light、point_lights、spot_lights、sky_light；可再扩展例如 `gi_probe_ids: Vec<u64>` 等，Lumelite 忽略即可。

- **ExtractedMeshes**：  
  - 保持 `meshes: HashMap<u64, ExtractedMesh>`；若未来需要「按批次/按材质」组织，可增加可选 `batches` 等，旧后端不读即可。

### 3.2 行为约定

- **「未实现」即忽略**：若某后端暂不支持某可选字段，则忽略该字段，不报错。例如 Lumelite 不支持 VG，则对 cluster 相关字段忽略；Lume 暂不支持 PBR 贴图，则对 material 忽略。
- **能力查询（可选）**：若宿主需要「根据后端能力决定是否填充某类数据」，可在 render-api 中增加可选 trait 方法，例如 `fn supports_pbr_materials(&self) -> bool`、`fn supports_virtual_geometry(&self) -> bool`，由各后端实现；默认可返回 false，避免强制所有后端实现。

---

## 4. 宿主侧：如何协调与切换

### 4.1 构造阶段

- **选 Lumelite**：`LumelitePlugin::new(device, queue)` 或 `LumeliteWindowBackend::from_window(window)`，得到 `Box<dyn RenderBackend>` 或 `Box<dyn RenderBackendWindow>`。  
- **选 Lume**：`LumePlugin::new(arc_dyn_device)`，得到同一 trait 对象。  
- 选择依据：配置（如 `render.backend = "lumelite" | "lume"`）、特性开关（如 `feature = "lume"`）、或运行时能力检测。

### 4.2 每帧逻辑（与后端无关）

```text
1. Extract：从主世界收集本帧的 mesh、相机、光源 → ExtractedMeshes、ExtractedView
2. 若当前后端支持 PBR 且场景有 PBR 材质，则填充 ExtractedMesh.material 等
3. 若当前后端为 Lume 且场景使用 VG，则填充 cluster 相关（未来）
4. backend.prepare(&extracted_meshes)
5. backend.render_frame(&view) 或 backend.render_frame_to_window(&view, ...)
```

同一套流程，不出现 `if lumelite { ... } else { ... }` 的渲染分支；仅「填充哪些可选字段」可根据后端能力做一次性配置或查询。

### 4.3 数据与资源

- **Lumelite**：顶点/索引来自 vertex_data/index_data；纹理由 prepare 阶段根据 material 上传并缓存（若采用 PBR 扩展）。  
- **Lume**：顶点/索引可同上，或从 VG Cluster 流送；GI 相关资源在 prepare 中由 Lume 内部管理。  
- 宿主只需保证「给到的 ExtractedMesh / ExtractedView 对当前后端语义正确」；不必关心底层是 wgpu 还是 lume-rhi。

---

## 5. 与「纯虚拟几何体 / 类 Lumen 光照」的衔接

- **纯虚拟几何体**：Lume 侧可将「传统 Mesh」仅用作回退或占位；主路径为 Cluster + Indirect Draw。render-api 的 ExtractedMesh 可保留为「回退或 LOD 表示」，未来若增加 `ExtractedClusterStream` 等，设为可选，Lumelite 忽略。  
- **类 Lumen 光照**：Lume 使用 ExtractedView 的光源与场景 SDF/Surface Cache 做光线追踪与累积；与 Lumelite 的「同一 ExtractedView」兼容，无需两套光源类型。  
- **风格差异**：  
  - Lumelite = 类 Flax：延迟、明确 GBuffer、多光源 Light Pass、PBR 贴图。  
  - Lume = 类 Lumen：VG + GI，可能无传统 GBuffer 或仅用于复合。  
  两者在 **render-api 层** 都只看到「Mesh（或 VG 占位）+ View + 光源」；底层实现完全分叉，通过「可选字段 + 忽略未实现」保持单一 API。

---

## 6. 小结

| 要点 | 做法 |
|------|------|
| **单一 API** | 继续以 render-api 为唯一契约；prepare / render_frame 不变。 |
| **可切换** | 宿主持有一个 `dyn RenderBackend`，构造时选 Lume 或 Lumelite；调用方式一致。 |
| **能力差异** | 通过「能力矩阵」与可选字段表达；各后端只消费自己支持的部分。 |
| **扩展** | 新增字段用 Option 或默认值；Lumelite 可加 PBR，Lume 可加 VG/GI 专用数据，互不破坏。 |
| **风格** | Lumelite 保持类 Flax 延迟渲染；Lume 保持纯 VG + 类 Lumen GI；两者在 API 层统一为「Extract → Prepare → Render」。 |

这样，**Lume 未来的纯虚拟几何体与类 Lumen 光照** 与 **Lumelite 的类 Flax 渲染风格** 可以良好地围绕单独一个 render-api 共存，并由宿主通过配置或能力选择可切换运行。
