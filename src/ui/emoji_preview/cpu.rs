use ratatui::text::Line;

use super::common::*;

pub fn render_billboard(
    texture: &Texture,
    width: usize,
    height: usize,
    tick: u64,
) -> Vec<Line<'static>> {
    let px_w = width;
    let px_h = height * 2;

    let tex_aspect = if texture.height > 0 {
        texture.width as f64 / texture.height as f64
    } else {
        1.0
    };

    let vp_aspect = px_w as f64 / px_h as f64;
    let fill = 0.65;
    let (board_w, board_h) = if tex_aspect > vp_aspect {
        let bw = px_w as f64 * fill;
        (bw, bw / tex_aspect)
    } else {
        let bh = px_h as f64 * fill;
        (bh * tex_aspect, bh)
    };
    let board_depth = board_w * 0.1;

    let half_w = board_w / 2.0;
    let half_h = board_h / 2.0;
    let half_d = board_depth / 2.0;

    let spin = tick as f64 * 0.04;
    let bob = (tick as f64 * 0.035).sin() * px_h as f64 * 0.03;
    let cos_s = spin.cos();
    let sin_s = spin.sin();

    let cx = px_w as f64 / 2.0;
    let cy = px_h as f64 / 2.0;

    let la = tick as f64 * 0.015;
    let light = normalize([la.cos() * 0.6, -0.5, la.sin() * 0.4 + 0.6]);

    let mut fb = vec![(0u8, 0u8, 0u8); px_w * px_h];
    let mut hit_mask = vec![false; px_w * px_h];

    background_gradient(&mut fb, px_w, px_h);

    let y_min = ((cy + bob - half_h).floor() as isize).max(0) as usize;
    let y_max = ((cy + bob + half_h).ceil() as usize + 1).min(px_h);

    let mut faces: [(u8, f64); 4] = [
        (0, half_d * cos_s),
        (1, -half_d * cos_s),
        (2, half_w * sin_s),
        (3, -half_w * sin_s),
    ];
    faces.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    for &(face_id, _) in &faces {
        for py in y_min..y_max {
            let world_y = py as f64 - (cy + bob);
            if world_y.abs() > half_h {
                continue;
            }
            let v_tex = (world_y + half_h) / (2.0 * half_h);

            for px_x in 0..px_w {
                let world_x = px_x as f64 - cx;

                let (r, g, b, a) = match face_id {
                    0 => {
                        if cos_s.abs() < 1e-8 { continue; }
                        let lx = (world_x + half_d * sin_s) / cos_s;
                        if lx.abs() > half_w { continue; }
                        let u = (lx + half_w) / (2.0 * half_w);
                        let [tr, tg, tb, ta] = texture.sample(u, v_tex);
                        if ta < 10 { continue; }
                        let n = [-sin_s, 0.0, cos_s];
                        let ndotl = dot(n, light).max(0.0);
                        let bright = 0.35 + ndotl * 0.65;
                        let spec = specular(n, light, 32.0) * 0.2;
                        (shade(tr, bright, spec), shade(tg, bright, spec), shade(tb, bright, spec), ta)
                    }
                    1 => {
                        if cos_s.abs() < 1e-8 { continue; }
                        let lx = (world_x - half_d * sin_s) / cos_s;
                        if lx.abs() > half_w { continue; }
                        let u = (-lx + half_w) / (2.0 * half_w);
                        let [tr, tg, tb, ta] = texture.sample(u, v_tex);
                        if ta < 10 { continue; }
                        let n = [sin_s, 0.0, -cos_s];
                        let ndotl = dot(n, light).max(0.0);
                        let bright = 0.35 + ndotl * 0.65;
                        let spec = specular(n, light, 32.0) * 0.2;
                        (shade(tr, bright, spec), shade(tg, bright, spec), shade(tb, bright, spec), ta)
                    }
                    2 => {
                        if sin_s.abs() < 1e-8 { continue; }
                        let lz = (half_w * cos_s - world_x) / sin_s;
                        if lz.abs() > half_d { continue; }
                        let [sr, sg, sb, sa] = texture.sample(0.99, v_tex);
                        if sa < 10 { continue; }
                        let n = [cos_s, 0.0, sin_s];
                        let ndotl = dot(n, light).max(0.0);
                        let bright = 0.25 + ndotl * 0.45;
                        ((sr as f64 * bright * 0.7) as u8, (sg as f64 * bright * 0.7) as u8, (sb as f64 * bright * 0.7) as u8, 255)
                    }
                    3 => {
                        if sin_s.abs() < 1e-8 { continue; }
                        let lz = (-half_w * cos_s - world_x) / sin_s;
                        if lz.abs() > half_d { continue; }
                        let [sr, sg, sb, sa] = texture.sample(0.01, v_tex);
                        if sa < 10 { continue; }
                        let n = [-cos_s, 0.0, -sin_s];
                        let ndotl = dot(n, light).max(0.0);
                        let bright = 0.25 + ndotl * 0.45;
                        ((sr as f64 * bright * 0.7) as u8, (sg as f64 * bright * 0.7) as u8, (sb as f64 * bright * 0.7) as u8, 255)
                    }
                    _ => continue,
                };

                let idx = py * px_w + px_x;
                hit_mask[idx] = true;
                let alpha = a as f64 / 255.0;
                let bg = fb[idx];
                let inv = 1.0 - alpha;
                fb[idx] = (
                    (r as f64 * alpha + bg.0 as f64 * inv) as u8,
                    (g as f64 * alpha + bg.1 as f64 * inv) as u8,
                    (b as f64 * alpha + bg.2 as f64 * inv) as u8,
                );
            }
        }
    }

    shadow_pass(&mut fb, &hit_mask, px_w, px_h);
    fb_to_lines(&fb, px_w, px_h, height)
}
