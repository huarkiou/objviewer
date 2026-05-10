// Blinn-Phong shader with 3-point lighting
// Matches the original Godot scene lighting setup

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) world_normal: vec3<f32>,
}

struct CameraUniform {
    view_proj: mat4x4<f32>,
    camera_position: vec3<f32>,
}

struct Light {
    position: vec3<f32>,
    color: vec3<f32>,
    energy: f32,
}

struct LightUniform {
    lights: array<Light, 3>,
    num_lights: u32,
    ambient: vec3<f32>,
}

struct MaterialUniform {
    albedo: vec3<f32>,
    metallic: f32,
    roughness: f32,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var<uniform> light: LightUniform;

@group(2) @binding(0)
var<uniform> material: MaterialUniform;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.world_position = in.position;
    out.world_normal = normalize(in.normal);
    return out;
}

@fragment
fn fs_main(in: VertexOutput, @builtin(front_facing) is_front: bool) -> @location(0) vec4<f32> {
    var N = normalize(in.world_normal);
    // Flip normal for back faces so both sides are lit correctly
    if (!is_front) {
        N = -N;
    }
    let V = normalize(camera.camera_position - in.world_position);

    var color = vec3<f32>(0.0);

    // Ambient
    color += light.ambient * material.albedo;

    // Point lights
    for (var i: u32 = 0u; i < light.num_lights; i++) {
        let L = normalize(light.lights[i].position - in.world_position);
        let H = normalize(L + V);

        // Diffuse (Lambert)
        let diff = max(dot(N, L), 0.0);

        // Specular (Blinn-Phong)
        let spec = pow(max(dot(N, H), 0.0), 8.0);

        color += light.lights[i].color * light.lights[i].energy * material.albedo * diff;
        color += light.lights[i].color * light.lights[i].energy * spec * 0.08;
    }

    return vec4<f32>(color, 1.0);
}
