# objviewer

[![CI](https://github.com/huarkiou/objviewer/actions/workflows/ci.yml/badge.svg)](https://github.com/huarkiou/objviewer/actions/workflows/ci.yml)

Lightweight Wavefront OBJ file viewer — single ~12 MB binary.

Amber-on-gray CAD-style rendering with sharp edge wireframe overlay.

## Usage

```
objviewer model.obj
```

| Control | Action |
|---------|--------|
| Left-drag | Rotate orbit |
| Scroll | Zoom in/out |
| ESC | Quit |

## Download

Pre-built binaries: [Releases](https://github.com/huarkiou/objviewer/releases)

| Platform | File |
|----------|------|
| Windows x64 | `objviewer-x86_64-pc-windows-msvc.zip` |
| Linux x64 | `objviewer-x86_64-unknown-linux-gnu.tar.gz` |
| macOS x64 | `objviewer-x86_64-apple-darwin.tar.gz` |

## Build from source

```
cargo build --release
```

Binary at `target/release/objviewer[.exe]`.

## Features

- Parse standard OBJ files (v/vn/vt/f), supports negative indices and line continuation
- Auto-normalize: model centered at origin, scaled to unit sphere
- Double-sided rendering with two-sided lighting — works for both solids and sheet bodies
- Sharp edge wireframe overlay (dihedral angle threshold 30°)
- Orbit camera with orthographic/perspective toggle
- 4× MSAA anti-aliasing
- CAD-style amber material with even flat lighting

## Interop with nozzle-design-rs

[nozzle-design-rs](https://github.com/huarkiou/nozzle-design-rs) 的 `sltn` 应用导出的 `.obj` 模型（`model.obj` / `downstream.obj` / `upstream.obj`）是本查看器的主要使用场景。

**兼容契约**：`sltn` 的 OBJ 导出仅使用本 parser 支持的 token 子集——`v`、`vn`、`f v//vn` 三角形面。

- 该契约由本仓库 `test_fixtures/sltn.obj`（nozzle-design-rs 输出的真实样本）与 `src/parser.rs` 的 `test_parse_sltn_fixture` 测试守护
- 若 `sltn` 导出格式变更（新增 token、四边形面、负索引等），本 parser 或 `sltn` 必须同步修改，且测试会首先失败
- `o`/`g`/`s`/`mtllib`/`usemtl` token 仅警告不报错；`cstype` 等曲线曲面 token 为致命错误

## Tech stack

| Crate | Version |
|-------|---------|
| winit | 0.30 |
| wgpu | 29 |
| egui | 0.34 |
| glam | 0.32 |

## License

MIT
