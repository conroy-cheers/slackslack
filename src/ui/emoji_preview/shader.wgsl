struct Uniforms {
    cos_rot: f32,
    sin_rot: f32,
    bob: f32,
    light_angle: f32,
    scale_x: f32,
    scale_y: f32,
    debug_flags: u32,
    _pad0: u32,
}

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var tex_sampler: sampler;
@group(1) @binding(2) var<uniform> edge_color: vec4f;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) normal: vec3f,
    @location(1) uv: vec2f,
    @location(2) @interpolate(flat) face_type: u32,
}

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
    @location(3) face_type: u32,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let cos_r = u.cos_rot;
    let sin_r = u.sin_rot;

    // Y-axis rotation
    let rx = in.position.x * cos_r - in.position.z * sin_r;
    let rz = in.position.x * sin_r + in.position.z * cos_r;
    let ry = in.position.y + u.bob;

    // Rotate normal
    let nx = in.normal.x * cos_r - in.normal.z * sin_r;
    let nz = in.normal.x * sin_r + in.normal.z * cos_r;

    var out: VertexOutput;
    out.position = vec4f(rx * u.scale_x, -ry * u.scale_y, rz * 0.001 + 0.5, 1.0);
    out.normal = vec3f(nx, in.normal.y, nz);
    out.uv = in.uv;
    out.face_type = in.face_type;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let la = u.light_angle;
    let light = normalize(vec3f(cos(la) * 0.6, -0.5, sin(la) * 0.4 + 0.6));
    let n = normalize(in.normal);

    var base_color: vec4f;
    var ambient: f32;
    var diff_strength: f32;

    if in.face_type <= 1u {
        let sample = textureSample(tex, tex_sampler, in.uv);
        if sample.a < 0.094 {
            discard;
        }
        base_color = vec4f(sample.rgb, 1.0);
        ambient = 0.35;
        diff_strength = 0.65;
    } else {
        let sample = textureSample(tex, tex_sampler, in.uv);
        base_color = vec4f(sample.rgb, 1.0);
        ambient = 0.25;
        diff_strength = 0.45;
    }

    if (u.debug_flags & 1u) != 0u {
        base_color = vec4f(vec3f(1.0), base_color.a);
    }

    let ndotl = max(dot(n, light), 0.0);
    let brightness = ambient + ndotl * diff_strength;

    // Specular (Blinn-Phong)
    let view = vec3f(0.0, 0.0, -1.0);
    let reflect = 2.0 * dot(n, light) * n - light;
    let spec = pow(max(-reflect.z, 0.0), 32.0) * 0.2;

    var dark_factor = 1.0;
    if in.face_type >= 2u {
        dark_factor = 0.7;
    }

    let rgb = base_color.rgb * brightness * dark_factor + vec3f(spec);
    return vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), base_color.a);
}
