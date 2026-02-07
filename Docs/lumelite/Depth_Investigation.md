# GBuffer 深度测试问题排查

## 现象
- `debug_direct_triangle=true`（直接画到 swapchain，无 depth）：三角形可见 ✓
- `debug_direct_triangle=false`（GBuffer→Present）：三角形不可见 ✗
- `debug_gbuffer_no_depth=true`（GBuffer 使用 depth_compare: Always）：三角形可见 ✓

## 结论
**根因**：GBuffer 的 depth 测试 (`Less` + clear 1.0) 导致片段被拒绝。

## 最终修复
1. 显式调用 `set_viewport(0, 0, w, h, 0.0, 1.0)` 确保标准深度范围
2. 使用 `depth_compare: LessEqual` 替代 `Less`，避免 NDC z 边界精度问题

以上修复已应用后，`debug_gbuffer_no_depth` 及其对应的 `pipeline_no_depth` 已移除，GBuffer 路径正常工作。
