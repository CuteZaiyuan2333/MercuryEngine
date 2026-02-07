# PBR 模型查看器（debug 示例）规划

本文档规划在 **debug** 目录下新增一个示例项目，使用 Lumelite 渲染带 PBR 贴图的 3D 模型（以椅子模型为例），并说明与现有 render-api 的协调方式。

---

## 1. 目标与测试资源

- **目标**：在 debug 中新增可执行程序（如 `pbr_model`），加载 OBJ 模型 + PBR 贴图，通过 Lumelite 的 GBuffer + Light Pass 渲染到窗口。
- **测试资源**：
  - 模型：`模型/green-vintage-metal-chair-with-books-and-flowers.obj`（同目录下有 `.mtl`）
  - 贴图目录：`模型/green-vintage-metal-chair-with-books-and-flowers/textures/`
    - `*_BaseColor.png`、`*_Normal.png`、`*_Metallic.png`、`*_Roughness.png`、`*_AO.png`（及可选 `*_Opacity.png`）
  - 若 OBJ 引用的 MTL 未指向该目录，可在加载时按「材质名 / 已知命名规则」映射到上述贴图。

---

## 2. 当前 Lumelite 能力与缺口

| 能力 | 现状 | PBR 需求 |
|------|------|----------|
| 顶点格式 | 仅 position + normal（24 字节），无 UV | 需要 **position + normal + uv**（至少 32 字节）以便采样贴图 |
| GBuffer 片段 | 写死常量（灰底、编码法线、固定粗糙度/金属度） | 需要按 **UV 采样** BaseColor、Normal、Roughness、Metallic、AO |
| 纹理绑定 | GBuffer Pass 无纹理，仅 view_proj + model uniform | 需要 **per-draw 绑定**：base_color、normal、metallic_roughness、ao 等纹理视图 |
| 数据流 | ExtractedMesh → vertex_data/index_data → MeshDraw | 若走统一 API：需在 Extract/Prepare 中携带「材质/纹理」信息；或先做「仅本示例」的扩展 |

---

## 3. 实现路径建议

### 方案 A：最小侵入（推荐先做）

- **位置**：在 **debug** 下新增 `pbr_model` bin；**lumelite-renderer** 内做「可选的 PBR 能力」扩展，**不**改 render-api 与 Bridge 的公共接口。
- **步骤**：
  1. **lumelite-renderer**  
     - 增加「第二种顶点布局」：例如 `VertexFormat::PositionNormalUv`（stride 32：pos 12 + normal 12 + uv 8）。  
     - 增加 **PBR GBuffer 变体**：新 WGSL（或同一 shader 用 `#ifdef`）支持在 fragment 中采样 base_color、normal、metallic_roughness、ao 四张纹理；GBufferPass 支持「无纹理 / 有纹理」两种 pipeline，或单独 `GBufferPbrPass`。  
     - 扩展 **MeshDraw** 或新增 **PbrMeshDraw**：在现有 `vertex_buf, index_buf, index_count, transform` 基础上，增加可选 `Option<PbrTextures>`（四张 TextureView 的引用或句柄）。  
  2. **Renderer**  
     - `encode_frame` 支持「混合 mesh 列表」：部分 MeshDraw 无纹理（沿用当前管线），部分带 PbrTextures（走 PBR GBuffer）；或首版只支持「全为 PBR」或「全为简单」两种模式。  
  3. **debug/pbr_model**  
     - 使用 `tobj`（或 `obj-rs`）加载 OBJ，解析顶点/索引及 UV；生成 **position + normal + uv** 的 `vertex_data`（与 lumelite-renderer 约定的 32 字节格式一致）。  
     - 使用 `image`（或 `png`）等解码贴图，通过 wgpu 上传为 Texture，得到 TextureView。  
     - 构造 **LumelitePlugin**（或直接持有一个 **Renderer** + 自己管理 swapchain）；每帧构建「带 PbrTextures 的 MeshDraw 列表」及 ExtractedView，调用扩展后的 encode_frame（或 renderer 上新接口），再 present。  
     - 资源路径：可硬编码或通过环境变量指向 `模型/green-vintage-metal-chair-with-books-and-flowers.obj` 及 `.../textures/`。  

- **优点**：不碰 render-api，不改变 Lume/Lumelite 切换契约；PBR 仅作为 Lumelite 的「扩展能力」，由需要 PBR 的宿主（如本示例）直接使用 renderer 的扩展接口。  
- **缺点**：不走「标准 prepare(rextracted) + render_frame(view)」的 PBR 路径；若将来要统一走 render-api，需再做阶段 2。

### 方案 B：从 render-api 起就支持「可选材质」

- **思路**：在 **render-api** 的 `ExtractedMesh` 中增加可选字段，例如 `material: Option<ExtractedPbrMaterial>`，其中 `ExtractedPbrMaterial` 可为「不透明句柄」或「贴图路径/ID」，由各后端自行解释；Lumelite 在 prepare 时上传并缓存纹理，render_frame 时按材质绑定。  
- **优点**：宿主统一通过 prepare + render_frame 即可驱动 PBR，后端可切换。  
- **缺点**：需设计跨后端的「材质抽象」（Lume 未来可能是 VG/GI 自己的材质，Lumelite 是贴图集），并约定「Lume 可忽略或映射」的语义；工作量较大，适合在方案 A 跑通后再做。

**建议**：先按 **方案 A** 落地 `pbr_model` 与 lumelite-renderer 的 PBR 扩展；验证椅子模型 + 整套 PBR 贴图在 Lumelite 下正确显示后，再视需要将「材质」抽象进 render-api（方案 B）。

---

## 4. 资源路径与贴图命名约定

- OBJ/MTL：`模型/green-vintage-metal-chair-with-books-and-flowers.obj`，`模型/green-vintage-metal-chair-with-books-and-flowers.mtl`。  
- 贴图目录：`模型/green-vintage-metal-chair-with-books-and-flowers/textures/`，当前命名示例：
  - `*_BaseColor.png`
  - `*_Normal.png`
  - `*_Metallic.png`
  - `*_Roughness.png`
  - `*_AO.png`
  - （可选）`*_Opacity.png`  

若 MTL 中未正确引用上述路径，pbr_model 可实现「按材质名或默认规则」映射到该目录下同名文件。

---

## 5. 与 Lume / 单一 render-api 的关系

- 本示例（pbr_model）**仅使用 Lumelite**，不涉及 Lume。  
- PBR 扩展若仅在 **lumelite-renderer** 内（方案 A），则 **render-api 与 Backend 切换逻辑不变**；Lume 侧无需实现「材质」相关字段。  
- 若后续采用方案 B，应在 render-api 中采用「可选扩展」设计：Lume 可忽略 `ExtractedMesh.material` 或将其映射为自身材质系统，从而保持「同一 Extract → prepare → render_frame」的可切换性。  

Lume 与 Lumelite 围绕单一 render-api 的协调与可切换架构，见 **[Render_API_Coordination.md](Render_API_Coordination.md)**。
