# Lessons Learned

## wgpu

### Depth buffer uses reversed-Z
glam 的 `perspective_rh` / `orthographic_rh` + OpenGL→wgpu NDC 转换矩阵后，实际深度值是反的：**近处深度大，远处深度小**。必须用 `CompareFunction::Greater`，clear 到 `0.0`。

### `InstanceDescriptor::default()` 在 wgpu 29 不存在
改用 `new_without_display_handle()` 或手动构造所有字段（`backends`, `flags`, `memory_budget_thresholds`, `backend_options`, `display`）。

### `SurfaceError` 在 wgpu 29 被移除
`get_current_texture()` 直接返回 `CurrentSurfaceTexture` 枚举，需要 match 处理 `Success` / `Suboptimal` / `Outdated` / `Lost` 等变体。

### `POLYGON_MODE_LINE` 不是所有后端都支持
DX12 需要额外 feature。更可移植的方案是用 `LineList` 拓扑 + 预生成边索引。

### 深度偏移 (depth bias) 只对三角形拓扑有效
`LineList` 不能用硬件 depth bias，只能在 vertex shader 里手动偏移 `clip_position.z`。

### wgpu 版本跳跃时 API 变化巨大
0.20 → 29 的变更包括：
- `entry_point` 从 `&str` 变成 `Option<&str>`
- `depth_write_enabled` / `depth_compare` 变成 `Option<T>`
- `multiview` → `multiview_mask: Option<NonZero<u32>>`
- `bind_group_layouts` 元素从 `&BGL` 变成 `Option<&BGL>`
- `push_constant_ranges` → `immediate_size`
- `RenderPassColorAttachment` 新增 `depth_slice`
- `RenderPassDescriptor` 新增 `multiview_mask`
- `request_device()` 少了一个参数

### MSAA 需要 resolve target
渲染到 multisampled texture，color attachment 设 `resolve_target` 指向 surface texture view，GPU 自动 resolve。

## egui

### `SidePanel` 在 egui 0.34 废弃
用 `Panel::left()` 替代。

### `show` → `show_inside` API 不兼容
`show_inside` 接受 `&mut Ui` 而非 `&Context`，不能简单替换。旧 API 仍可用但需 `#[allow(deprecated)]`。

## Rust

### edition 2024 兼容性
winit 0.30 + wgpu 29 + egui 0.34 在 edition 2024 下编译无问题。

### `#[cfg]` 在 `let` 语句上
```rust
#[cfg(target_os = "windows")]
let backends = wgpu::Backends::DX12;
#[cfg(not(target_os = "windows"))]
let backends = wgpu::Backends::VULKAN;
```
互斥的 cfg 条件可以在同一作用域内声明同名变量。

## 渲染

### 双面渲染需要 `@builtin(front_facing)` 翻转法线
片体查看时必须 `cull_mode: None` + 在 fragment shader 里检测 `is_front` 并翻转背面法线，否则背面全黑。

### 锐边检测 vs 轮廓检测
- 锐边（dihedral angle）：静态，不随视角变化，适合展示几何特征
- 轮廓（silhouette）：依赖视角，需要每帧重算，适合展示外形

### 球心归一化
中心移到原点 + 缩放到单位球（半径 1），比单一轴向归一化更通用，任何形状的模型都能正常查看。
