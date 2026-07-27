struct Camera {
    view_projection: mat4x4<f32>,
    camera_position: vec4<f32>,
    camera_forward: vec4<f32>,
    fog: vec4<f32>,
    fog_color: vec4<f32>,
    ambient: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct Material {
    base_color: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> material: Material;

@group(2) @binding(0)
var texture_sampler: sampler;

@group(2) @binding(1)
var block_texture: texture_2d<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) ao: f32,
    @location(3) tint: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) ao: f32,
    @location(2) tint: vec3<f32>,
    @location(3) fog_amount: f32,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = camera.view_projection * vec4<f32>(input.position, 1.0);
    output.tex_coords = input.uv;
    output.ao = input.ao;
    output.tint = input.tint;
    let eye_depth = max(dot(input.position - camera.camera_position.xyz, camera.camera_forward.xyz), 0.0);
    let linear_fog = clamp(eye_depth / camera.fog.x, 0.0, 1.0);
    let exponential_fog = 1.0 - exp(-camera.fog.y * eye_depth);
    output.fog_amount = select(linear_fog, exponential_fog, camera.fog.z > 0.5);
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(block_texture, texture_sampler, input.tex_coords);
    let color = tex_color * material.base_color;
    if (color.a <= 0.0001) {
        discard;
    }
    let lit_color = color.rgb * input.tint * input.ao * camera.ambient.rgb;
    return vec4<f32>(mix(lit_color, camera.fog_color.rgb, input.fog_amount), color.a);
}
