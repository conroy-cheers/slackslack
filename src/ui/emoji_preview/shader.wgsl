struct Uniforms {
    mvp: mat4x4f,
    normal_rot: mat4x4f,
    shadow_mvp: mat4x4f,
    ground_mvp: mat4x4f,
    light_dir: vec4f,
    bg_color: vec4f,
    camera_pos: vec4f,
    ground_y: f32,
    debug_flags: u32,
    near: f32,
    far: f32,
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

struct SceneOutput {
    @location(0) color: vec4f,
    @location(1) depth: f32,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let world_pos = u.mvp * vec4f(in.position, 1.0);
    let rotated_normal = (u.normal_rot * vec4f(in.normal, 0.0)).xyz;

    var out: VertexOutput;
    out.position = world_pos;
    out.normal = rotated_normal;
    out.uv = in.uv;
    out.face_type = in.face_type;
    return out;
}

fn luminance(c: vec3f) -> f32 {
    return dot(c, vec3f(0.299, 0.587, 0.114));
}

fn perturb_normal(geom_n: vec3f, uv: vec2f, face_type: u32) -> vec3f {
    let tex_size = vec2f(textureDimensions(tex));
    let texel = 1.0 / tex_size;

    let h_l = luminance(textureSample(tex, tex_sampler, uv + vec2f(-texel.x, 0.0)).rgb);
    let h_r = luminance(textureSample(tex, tex_sampler, uv + vec2f( texel.x, 0.0)).rgb);
    let h_d = luminance(textureSample(tex, tex_sampler, uv + vec2f(0.0, -texel.y)).rgb);
    let h_u = luminance(textureSample(tex, tex_sampler, uv + vec2f(0.0,  texel.y)).rgb);

    let du = (h_r - h_l) * 0.5;
    let dv = (h_u - h_d) * 0.5;
    let strength = 3.0;

    // Tangent-space perturbation: front face tangent = +X, bitangent = -Y
    // Back face mirrors U, so flip the tangent
    var tu = du * strength;
    if face_type == 1u {
        tu = -tu;
    }
    let tv = -dv * strength;

    let up = select(vec3f(1.0, 0.0, 0.0), vec3f(0.0, 1.0, 0.0), abs(geom_n.y) < 0.99);
    let tangent = normalize(cross(up, geom_n));
    let bitangent = cross(geom_n, tangent);

    return normalize(geom_n + tangent * tu + bitangent * tv);
}

@fragment
fn fs_main(in: VertexOutput) -> SceneOutput {
    let light = normalize(u.light_dir.xyz);
    let geom_n = normalize(in.normal);

    var base_color: vec4f;
    var ambient: f32;
    var diff_strength: f32;

    let sample = textureSample(tex, tex_sampler, in.uv);
    base_color = vec4f(sample.rgb, 1.0);

    if in.face_type <= 1u {
        ambient = 0.35;
        diff_strength = 0.65;
    } else {
        ambient = 0.25;
        diff_strength = 0.45;
    }

    if (u.debug_flags & 1u) != 0u {
        base_color = vec4f(vec3f(1.0), base_color.a);
    }

    let n = normalize(perturb_normal(geom_n, in.uv, in.face_type));

    let ndotl = max(dot(n, light), 0.0);
    let brightness = ambient + ndotl * diff_strength;

    let reflect = 2.0 * dot(n, light) * n - light;
    let view = normalize(u.camera_pos.xyz);
    let lum = luminance(base_color.rgb);
    let spec = pow(max(dot(reflect, view), 0.0), 32.0) * 0.2 * smoothstep(0.3, 0.8, lum);

    var dark_factor = 1.0;
    if in.face_type >= 2u {
        dark_factor = 0.7;
    }

    let rgb = base_color.rgb * brightness * dark_factor + vec3f(spec);
    var out: SceneOutput;
    out.color = vec4f(clamp(rgb, vec3f(0.0), vec3f(1.0)), base_color.a);
    out.depth = in.position.z;
    return out;
}

struct ShadowVertexOutput {
    @builtin(position) position: vec4f,
}

@vertex
fn vs_shadow(in: VertexInput) -> ShadowVertexOutput {
    var out: ShadowVertexOutput;
    out.position = u.shadow_mvp * vec4f(in.position, 1.0);
    return out;
}

@fragment
fn fs_shadow(@builtin(position) pos: vec4f) -> SceneOutput {
    var out: SceneOutput;
    out.color = vec4f(0.0, 0.0, 0.0, 0.45);
    out.depth = pos.z;
    return out;
}

struct GroundVertexOutput {
    @builtin(position) position: vec4f,
    @location(0) world_xz: vec2f,
}

@vertex
fn vs_ground(@builtin(vertex_index) index: u32) -> GroundVertexOutput {
    let extent = 500.0;
    var corners = array<vec2f, 4>(
        vec2f(-extent, -extent),
        vec2f( extent, -extent),
        vec2f( extent,  extent),
        vec2f(-extent,  extent),
    );
    var indices = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    let vi = indices[index];
    let xz = corners[vi];
    let world = vec4f(xz.x, u.ground_y, xz.y, 1.0);

    var out: GroundVertexOutput;
    out.position = u.ground_mvp * world;
    out.world_xz = xz;
    return out;
}

@fragment
fn fs_ground(in: GroundVertexOutput) -> SceneOutput {
    let dist = length(in.world_xz);
    let t = clamp(dist / 4.0, 0.0, 1.0);
    let center = u.bg_color.rgb * 2.3;
    let edge = u.bg_color.rgb;
    var out: SceneOutput;
    out.color = vec4f(mix(center, edge, t), 1.0);
    out.depth = in.position.z;
    return out;
}

struct SsaoParams {
    strength: f32,
    depth_threshold: f32,
    start_dist: f32,
    step_growth: f32,
    max_shadow: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var<uniform> ssao_u: Uniforms;
@group(0) @binding(1) var ssao_color: texture_2d<f32>;
@group(0) @binding(2) var ssao_linear_depth: texture_2d<f32>;
@group(0) @binding(3) var<uniform> ssao_params: SsaoParams;

@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    var positions = array<vec2f, 3>(
        vec2f(-1.0, -3.0),
        vec2f(-1.0, 1.0),
        vec2f(3.0, 1.0),
    );
    return vec4f(positions[index], 0.0, 1.0);
}

fn linearize_depth(d: f32) -> f32 {
    // Reverse the perspective depth: z_clip = (far * z_eye + near * far) / (-z_eye * (far - near))
    // Solving for z_eye (positive distance from camera):
    return ssao_u.near * ssao_u.far / (ssao_u.far - d * (ssao_u.far - ssao_u.near));
}

@fragment
fn fs_ssao(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let px = vec2i(frag_coord.xy);
    let color = textureLoad(ssao_color, px, 0);
    let raw_depth = textureLoad(ssao_linear_depth, px, 0).r;

    if raw_depth >= 0.999 {
        return color;
    }

    let my_lin = linearize_depth(raw_depth);
    let dims = vec2f(textureDimensions(ssao_color));

    // Project light direction into screen space.
    // Transform the direction as (w=0) through view-projection, then convert
    // the resulting clip-space vector to pixel offsets.
    let p0 = ssao_u.ground_mvp * vec4f(0.0, 0.0, 0.0, 1.0);
    let p1 = ssao_u.ground_mvp * vec4f(ssao_u.light_dir.xyz, 1.0);
    let s0 = p0.xy / p0.w;
    let s1 = p1.xy / p1.w;
    var light_ndc = s1 - s0;
    // NDC Y+ is up but frag_coord Y+ is down — flip Y
    light_ndc.y = -light_ndc.y;
    let light_screen = normalize(light_ndc * dims);

    var occlusion = 0.0;
    var total_weight = 0.0;
    let steps = 48;
    var dist = ssao_params.start_dist;
    let growth = ssao_params.step_growth;
    for (var i = 0; i < steps; i++) {
        let offset = light_screen * dist;
        let sample_px = vec2i(frag_coord.xy + offset);
        if sample_px.x < 0 || sample_px.y < 0 || f32(sample_px.x) >= dims.x || f32(sample_px.y) >= dims.y {
            dist *= growth;
            continue;
        }
        let sample_raw = textureLoad(ssao_linear_depth, sample_px, 0).r;
        if sample_raw >= 0.999 {
            dist *= growth;
            continue;
        }
        let sample_lin = linearize_depth(sample_raw);
        let weight = 1.0 / (1.0 + dist * 0.008);
        total_weight += weight;
        if sample_lin < my_lin - ssao_params.depth_threshold {
            occlusion += weight;
        }
        dist *= growth;
    }

    let depth_fade = 1.0 - smoothstep(0.9, 0.999, raw_depth);
    let raw_occ = select(0.0, occlusion / total_weight, total_weight > 0.0);
    let shadow = clamp(raw_occ * ssao_params.strength, 0.0, ssao_params.max_shadow) * depth_fade;
    return vec4f(color.rgb * (1.0 - shadow), color.a);
}

struct PostprocessUniforms {
    contrast: f32,
    sharpen: f32,
    dither: f32,
    frame: f32,
    vhs: f32,
    _pp_pad0: f32,
    _pp_pad1: f32,
    _pp_pad2: f32,
}

@group(0) @binding(0) var<uniform> pp: PostprocessUniforms;
@group(0) @binding(1) var pp_input: texture_2d<f32>;

fn hash_noise(p: vec2f) -> f32 {
    return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453);
}

fn tri_dither(coord: vec2f, frame: f32) -> vec3f {
    let n0 = hash_noise(coord + vec2f(frame * 1.37, frame * 0.71));
    let n1 = hash_noise(coord + vec2f(frame * 2.13, frame * 1.93) + vec2f(0.5));
    return vec3f(n0 + n1 - 1.0);
}

fn apply_contrast(c: vec3f) -> vec3f {
    return clamp((c - vec3f(0.5)) * pp.contrast + vec3f(0.5), vec3f(0.0), vec3f(1.0));
}

fn load_pp(coord: vec2i, dims: vec2i) -> vec3f {
    return apply_contrast(textureLoad(pp_input, clamp(coord, vec2i(0), dims - 1), 0).rgb);
}

@fragment
fn fs_postprocess(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let px = vec2i(frag_coord.xy);
    let dims = vec2i(textureDimensions(pp_input));
    var color = load_pp(px, dims);

    if pp.sharpen > 0.0 {
        let l = load_pp(px + vec2i(-1, 0), dims);
        let r = load_pp(px + vec2i( 1, 0), dims);
        let t = load_pp(px + vec2i( 0,-1), dims);
        let b = load_pp(px + vec2i( 0, 1), dims);
        let avg = (l + r + t + b) * 0.25;
        color = clamp(color + (color - avg) * pp.sharpen, vec3f(0.0), vec3f(1.0));
    }

    if pp.vhs > 0.0 {
        let row = f32(px.y);
        let time = pp.frame;

        let row_noise = hash_noise(vec2f(row * 0.37, time * 0.13));
        let blur_w = pp.vhs * (1.0 + row_noise * 2.0);
        let kernel = i32(ceil(blur_w));
        var blurred = vec3f(0.0);
        var total_w = 0.0;
        for (var dx = -kernel; dx <= kernel; dx++) {
            let w = max(0.0, 1.0 - abs(f32(dx)) / blur_w);
            blurred += load_pp(px + vec2i(dx, 0), dims) * w;
            total_w += w;
        }
        blurred /= total_w;

        let fringe = i32(ceil(pp.vhs * 1.8));
        let cr = load_pp(px + vec2i(fringe, 0), dims).r;
        let cb = load_pp(px + vec2i(-fringe, 0), dims).b;
        blurred = vec3f(
            mix(blurred.r, cr, pp.vhs * 0.55),
            blurred.g,
            mix(blurred.b, cb, pp.vhs * 0.55),
        );

        let odd = f32(px.y % 2);
        let scanline_gap = 1.0 - pp.vhs * 0.20 * odd;
        let scanline_wave = 1.0 - pp.vhs * 0.02 * (1.0 + sin(row * 0.15 + time * 0.3));

        color = clamp(blurred * scanline_gap * scanline_wave, vec3f(0.0), vec3f(1.0));
    }

    if pp.dither > 0.0 {
        let noise = tri_dither(frag_coord.xy, pp.frame) * pp.dither / 32.0;
        color = clamp(color + noise, vec3f(0.0), vec3f(1.0));
    }

    return vec4f(color, 1.0);
}

@group(0) @binding(0) var downsample_input: texture_2d<f32>;

@fragment
fn fs_downsample(@builtin(position) frag_coord: vec4f) -> @location(0) vec4f {
    let dst = vec2i(frag_coord.xy);
    let src = dst * 2;
    let dims = vec2i(textureDimensions(downsample_input));
    let a = textureLoad(downsample_input, clamp(src, vec2i(0), dims - 1), 0).rgb;
    let b = textureLoad(downsample_input, clamp(src + vec2i(1, 0), vec2i(0), dims - 1), 0).rgb;
    let c = textureLoad(downsample_input, clamp(src + vec2i(0, 1), vec2i(0), dims - 1), 0).rgb;
    let d = textureLoad(downsample_input, clamp(src + vec2i(1, 1), vec2i(0), dims - 1), 0).rgb;
    return vec4f((a + b + c + d) * 0.25, 1.0);
}
