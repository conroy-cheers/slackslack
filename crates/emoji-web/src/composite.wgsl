struct Uniforms {
    output_size: vec2f,
    time_secs: f32,
    preview_mix: f32,
    terminal_rect: vec4f,
    overlay_uv_rect: vec4f,
    billboard_rect: vec4f,
    terminal_grid: vec4f,
    transfer_tuning: vec4f,
    perf_toggles: vec4f,
    channel_switch: vec4f,
}

@group(0) @binding(0) var overlay_tex: texture_2d<f32>;
@group(0) @binding(1) var overlay_sampler: sampler;
@group(0) @binding(2) var<uniform> u: Uniforms;
@group(0) @binding(3) var billboard_tex: texture_2d<f32>;
@group(0) @binding(4) var billboard_sampler: sampler;

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

fn sample_filtered(
    tex: texture_2d<f32>,
    samp: sampler,
    uv: vec2f,
    texel: vec2f,
) -> vec4f {
    let center = textureSampleLevel(tex, samp, uv, 0.0) * 0.40;
    let horiz =
        textureSampleLevel(tex, samp, uv + vec2f(texel.x, 0.0), 0.0) * 0.15 +
        textureSampleLevel(tex, samp, uv - vec2f(texel.x, 0.0), 0.0) * 0.15;
    let vert =
        textureSampleLevel(tex, samp, uv + vec2f(0.0, texel.y), 0.0) * 0.15 +
        textureSampleLevel(tex, samp, uv - vec2f(0.0, texel.y), 0.0) * 0.15;
    return center + horiz + vert;
}

fn sample_billboard_scene(uv: vec2f) -> vec3f {
    return textureSampleLevel(
        billboard_tex,
        billboard_sampler,
        clamp(uv, vec2f(0.0), vec2f(1.0)),
        0.0,
    ).rgb;
}

fn hash12(p: vec2f) -> f32 {
    let h = dot(p, vec2f(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

struct VsOut {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    var positions = array<vec2f, 3>(
        vec2f(-1.0, -3.0),
        vec2f(-1.0, 1.0),
        vec2f(3.0, 1.0),
    );
    var uvs = array<vec2f, 3>(
        vec2f(0.0, 2.0),
        vec2f(0.0, 0.0),
        vec2f(2.0, 0.0),
    );

    var out: VsOut;
    out.position = vec4f(positions[index], 0.0, 1.0);
    out.uv = uvs[index];
    return out;
}

@fragment
fn fs_screen(in: VsOut) -> @location(0) vec4f {
    let term_uv = clamp(in.uv, vec2f(0.0), vec2f(1.0));
    let overlay_texel = 1.0 / vec2f(textureDimensions(overlay_tex));
    var overlay = textureSampleLevel(overlay_tex, overlay_sampler, term_uv, 0.0);
    if u.perf_toggles.z > 0.5 {
        overlay = sample_filtered(overlay_tex, overlay_sampler, term_uv, overlay_texel * 0.65);
    }

    var color = overlay.rgb;
    if u.perf_toggles.x > 0.5 {
        let switch_phase = 1.0 - abs(u.preview_mix * 2.0 - 1.0);
        let switching = f32(u.preview_mix > 0.001 && u.preview_mix < 0.999);
        let crt_strength = 1.0 - u.preview_mix;
        let aperture = mix(1.10, 0.015, switch_phase * switch_phase);
        let band_dist = abs(term_uv.y - 0.5);
        let visible = 1.0 - smoothstep(aperture, aperture + 0.015, band_dist);
        let glow = exp(-band_dist * 140.0) * switch_phase;
        let switched = color * visible + vec3f(1.0, 0.98, 0.90) * glow * 0.35;
        color = mix(color, switched, switching);

        let overlay_dims = vec2f(textureDimensions(overlay_tex));
        let source_cell_h = max(overlay_dims.y / max(u.terminal_grid.y, 1.0), 1.0);
        let source_row_pos = (term_uv.y * overlay_dims.y) / source_cell_h;
        let row_phase = fract(source_row_pos);
        let row_center_dist = abs(row_phase - 0.5);
        let row_core = 1.0 - smoothstep(0.10, 0.46, row_center_dist);
        let scan = 1.0 - crt_strength * (0.34 - row_core * 0.34);
        let beam = 1.0 - crt_strength * 0.06 + crt_strength * 0.06 * sin((floor(source_row_pos) + 0.5) * 0.9 + u.time_secs * 0.9);
        let phosphor_gain = 1.0 + row_core * 0.16 * crt_strength;
        let flicker = 1.0 - crt_strength * 0.02 + crt_strength * 0.02 * sin(u.time_secs * 37.0 + term_uv.y * 11.0);
        let edge =
            smoothstep(0.0, 0.08, term_uv.x) *
            smoothstep(0.0, 0.08, term_uv.y) *
            smoothstep(0.0, 0.08, 1.0 - term_uv.x) *
            smoothstep(0.0, 0.08, 1.0 - term_uv.y);
        color *= scan * beam * flicker * phosphor_gain;
        color = mix(color * (1.0 - 0.12 * crt_strength), color, edge);
    }

    color = clamp(color, vec3f(0.0), vec3f(1.0));
    if u.perf_toggles.y > 0.5 {
        color = apply_transfer_tuning(color);
    }

    return vec4f(color, overlay.a);
}

@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4f {
    let frag_px = in.uv * u.output_size;
    let channel = u.channel_switch.x;
    let channel_dir = select(-1.0, 1.0, u.channel_switch.y >= 0.0);
    let bg_t = clamp(length((frag_px - u.output_size * 0.5) / u.output_size), 0.0, 1.0);
    let gallery_bg = mix(vec3f(0.01, 0.06, 0.03), vec3f(0.0, 0.0, 0.0), bg_t);
    let preview_bg = mix(vec3f(0.015, 0.015, 0.02), vec3f(0.0, 0.0, 0.0), bg_t);
    var color = mix(gallery_bg, preview_bg, u.preview_mix);

    let row_wobble =
        (sin(in.uv.y * 180.0 + u.time_secs * 44.0) * 0.018 +
         sin(in.uv.y * 37.0 - u.time_secs * 19.0) * 0.008 +
         channel_dir * (in.uv.y - 0.5) * 0.03) * channel;

    let local = vec2f(frag_px.x + row_wobble * u.billboard_rect.z, frag_px.y) - u.billboard_rect.xy;
    let bb_uv = vec2f(
        clamp(local.x / max(u.billboard_rect.z, 1.0), 0.0, 1.0),
        clamp(local.y / max(u.billboard_rect.w, 1.0), 0.0, 1.0),
    );
    let inside = u.billboard_rect.z > 0.0 && u.billboard_rect.w > 0.0
        && local.x >= 0.0 && local.y >= 0.0
        && local.x < u.billboard_rect.z && local.y < u.billboard_rect.w;
    if inside && u.preview_mix > 0.0 {
        let bb = sample_billboard_scene(bb_uv);
        color = mix(color, bb, u.preview_mix);
    }

    let term_local = vec2f(frag_px.x + row_wobble * u.terminal_rect.z, frag_px.y) - u.terminal_rect.xy;
    let term_local_uv = vec2f(
        clamp(term_local.x / max(u.terminal_rect.z, 1.0), 0.0, 1.0),
        clamp(term_local.y / max(u.terminal_rect.w, 1.0), 0.0, 1.0),
    );
    let term_uv = u.overlay_uv_rect.xy + term_local_uv * u.overlay_uv_rect.zw;
    let in_term = term_local.x >= 0.0 && term_local.y >= 0.0
        && term_local.x < u.terminal_rect.z && term_local.y < u.terminal_rect.w;
    if in_term {
        let screen = textureSampleLevel(overlay_tex, overlay_sampler, term_uv, 0.0);
        color = mix(color, screen.rgb, screen.a);
    }

    if channel > 0.0 {
        let noise_coord = floor(vec2f(in.uv.x * u.output_size.x * 0.65, in.uv.y * u.output_size.y * 0.35) + vec2f(u.time_secs * 120.0, u.time_secs * 53.0));
        let noise = hash12(noise_coord);
        let burst = smoothstep(0.25, 1.0, sin(in.uv.y * 96.0 - u.time_secs * 31.0) * 0.5 + 0.5);
        let static_mix = channel * (0.30 + burst * 0.28);
        let monochrome = vec3f(dot(color, vec3f(0.2126, 0.7152, 0.0722)));
        color = mix(color, monochrome, channel * 0.45);
        color = mix(color, vec3f(noise), static_mix * 0.55);
        color *= 1.0 - channel * 0.08 + burst * channel * 0.22;
    }

    return vec4f(clamp(color, vec3f(0.0), vec3f(1.0)), 1.0);
}
