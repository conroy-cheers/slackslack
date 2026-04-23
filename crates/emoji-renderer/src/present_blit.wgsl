struct Uniforms {
    output_size: vec2f,
    opacity: f32,
    apply_transfer: f32,
    dest_rect: vec4f,
    uv_rect: vec4f,
    transfer_tuning: vec4f,
    extra_params: vec4f,
}

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;
@group(0) @binding(2) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

fn quantize_u8(c: f32) -> f32 {
    return floor(clamp(c, 0.0, 1.0) * 255.0 + 0.5) / 255.0;
}

fn linear_to_srgb_channel_exact(c: f32) -> f32 {
    let q = quantize_u8(c);
    var s = q * 12.92;
    if q > 0.0031308 {
        s = 1.055 * pow(max(q, 0.0), 1.0 / 2.4) - 0.055;
    }
    return quantize_u8(s);
}

fn linear_to_srgb(c: vec3f) -> vec3f {
    return vec3f(
        linear_to_srgb_channel_exact(c.r),
        linear_to_srgb_channel_exact(c.g),
        linear_to_srgb_channel_exact(c.b),
    );
}

fn apply_transfer_tuning(c: vec3f) -> vec3f {
    let gain = max(u.transfer_tuning.x, 0.0);
    let gamma = max(u.transfer_tuning.y, 0.01);
    let lift = u.transfer_tuning.z;
    let saturation = max(u.transfer_tuning.w, 0.0);
    let gained = clamp(c * gain, vec3f(0.0), vec3f(1.0));
    let encoded = linear_to_srgb(gained);
    let lifted = clamp(encoded + vec3f(lift), vec3f(0.0), vec3f(1.0));
    let gamma_corrected = clamp(vec3f(
        pow(lifted.r, 1.0 / gamma),
        pow(lifted.g, 1.0 / gamma),
        pow(lifted.b, 1.0 / gamma),
    ), vec3f(0.0), vec3f(1.0));
    let luma = dot(gamma_corrected, vec3f(0.2126, 0.7152, 0.0722));
    return clamp(mix(vec3f(luma), gamma_corrected, saturation), vec3f(0.0), vec3f(1.0));
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    var quad = array<vec2f, 6>(
        vec2f(0.0, 0.0),
        vec2f(1.0, 0.0),
        vec2f(1.0, 1.0),
        vec2f(0.0, 0.0),
        vec2f(1.0, 1.0),
        vec2f(0.0, 1.0),
    );
    let p = quad[index];
    let pixel = u.dest_rect.xy + p * u.dest_rect.zw;
    let ndc = vec2f(
        (pixel.x / u.output_size.x) * 2.0 - 1.0,
        1.0 - (pixel.y / u.output_size.y) * 2.0,
    );
    let src_p = vec2f(p.x, select(p.y, 1.0 - p.y, u.extra_params.x > 0.5));
    var out: VsOut;
    out.position = vec4f(ndc, 0.0, 1.0);
    out.uv = u.uv_rect.xy + src_p * u.uv_rect.zw;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4f {
    let sample = textureSampleLevel(src_tex, src_sampler, in.uv, 0.0);
    let color = select(sample.rgb, apply_transfer_tuning(sample.rgb), u.apply_transfer > 0.5);
    return vec4f(color, sample.a * u.opacity);
}
