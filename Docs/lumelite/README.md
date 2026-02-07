# Lumelite 文档索引

Lumelite 是 **Lume 渲染引擎的轻量兼容子集**：与 Lume 接口兼容、架构相通，但不包含虚拟几何体 (Virtual Geometry) 与全局光照 (Global Illumination)。本目录包含 Lumelite 的架构与开发文档。

## 文档列表

| 文档 | 说明 |
|------|------|
| [Architecture_Overview.md](Architecture_Overview.md) | 总体架构：定位、分层（RHI / Renderer / Bridge）、与 Lume 的差异 |
| [Project_Structure.md](Project_Structure.md) | 项目目录结构：各 crate 职责、模块划分、virtual_geom/gi 在 Lumelite 中的处理方式 |
| [Development_Roadmap.md](Development_Roadmap.md) | 详细开发路线图：阶段〇～五的任务、验收标准与依赖关系 |
| [Interface_Compatibility.md](Interface_Compatibility.md) | 与 Lume 的接口兼容说明及从 Lumelite 迁移到 Lume 的步骤 |
| [Current_Status.md](Current_Status.md) | 当前代码状态与相对路线图的缺口总结 |
| [Backend_Switch.md](Backend_Switch.md) | **与 Lume 切换**：render-api、RenderBackend 统一接口及宿主用法 |
| [Backend_Agnostic_Analysis.md](Backend_Agnostic_Analysis.md) | **后端无关渲染**：宿主只调 render-api、不直接调用 wgpu 的需求分析与实现路径 |
| [PBR_Model_Viewer_Plan.md](PBR_Model_Viewer_Plan.md) | **PBR 模型查看器**：debug 下 pbr_model 示例规划、测试资源（OBJ+贴图）与实现路径 |
| [Render_API_Coordination.md](Render_API_Coordination.md) | **Lume / Lumelite 协调**：围绕单一 render-api 的可切换架构与能力矩阵 |

## 快速参考

- **代码位置**：Lumelite 源码位于仓库根目录下的 `lumelite/`（与 `lume/` 并列）。共享类型与后端 trait 位于根目录 `render-api/`。示例与调试程序位于根目录 `debug/`。
- **技术选型**：**wgpu**（不使用 Vulkan/Metal 直连）；**延迟光照**（GBuffer + 多光源 Light Pass、Lambert + GGX/Schlick/Smith）。见本目录架构与路线图文档。
- **与 Lume 的关系**：通过 **render-api** 与 **RenderBackend** 统一接口；宿主只依赖 render-api，可选用 Lume 或 Lumelite 实现，同一套 prepare/render_frame 调用。详见 [Backend_Switch.md](Backend_Switch.md)。
- **ExtractedView**：支持 `directional_light`（可选）、`point_lights`、`spot_lights`、`sky_light`；宿主可配置多光源。
- **当前能力**：方向光/点光/聚光、Shadow Map（单 cascade）、Surface 缓存、Resize 与 SurfaceError 处理、uniform 复用；gbuffer_light_window 透视投影 + 窗口流畅渲染。
