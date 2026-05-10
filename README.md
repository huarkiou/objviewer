# objviewer

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

## Tech stack

| Crate | Version |
|-------|---------|
| winit | 0.30 |
| wgpu | 29 |
| egui | 0.34 |
| glam | 0.32 |

## License

MIT
