// Simple constant-color shader for wireframe edge overlay
// Shares the same CameraUniform bind group (group 0) as the main shader

struct CameraUniform {
    view_proj: mat4x4<f32>,
    camera_position: vec3<f32>,
}

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    // Manual depth offset — larger value to ensure lines render in front of solid
    // Minimal depth offset to prevent z-fighting.
    // Depth is reversed: larger depth = closer to camera.
    out.clip_position.z += 0.00001 * out.clip_position.w;
    return out;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.12, 0.12, 0.12, 1.0);
}
