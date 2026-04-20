use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

pub struct Texture<'a> {
    pub pixels: &'a [[u8; 4]],
    pub width: u32,
    pub height: u32,
}

impl Texture<'_> {
    pub fn sample(&self, u: f64, v: f64) -> [u8; 4] {
        if self.width == 0 || self.height == 0 {
            return [0, 0, 0, 0];
        }
        let x = ((u.clamp(0.0, 0.9999) * self.width as f64) as u32).min(self.width - 1);
        let y = ((v.clamp(0.0, 0.9999) * self.height as f64) as u32).min(self.height - 1);
        let idx = (y * self.width + x) as usize;
        self.pixels.get(idx).copied().unwrap_or([0, 0, 0, 0])
    }

    pub fn edge_color(&self) -> [u8; 3] {
        let (mut r, mut g, mut b, mut count) = (0u64, 0u64, 0u64, 0u64);
        let (w, h) = (self.width, self.height);
        if w == 0 || h == 0 {
            return [80, 80, 80];
        }
        for x in 0..w {
            for &y in &[0, h - 1] {
                let idx = (y * w + x) as usize;
                if let Some(&[pr, pg, pb, pa]) = self.pixels.get(idx) {
                    if pa > 128 {
                        r += pr as u64;
                        g += pg as u64;
                        b += pb as u64;
                        count += 1;
                    }
                }
            }
        }
        for y in 0..h {
            for &x in &[0, w - 1] {
                let idx = (y * w + x) as usize;
                if let Some(&[pr, pg, pb, pa]) = self.pixels.get(idx) {
                    if pa > 128 {
                        r += pr as u64;
                        g += pg as u64;
                        b += pb as u64;
                        count += 1;
                    }
                }
            }
        }
        if count == 0 {
            return [80, 80, 80];
        }
        [(r / count) as u8, (g / count) as u8, (b / count) as u8]
    }
}

pub fn background_gradient(fb: &mut [(u8, u8, u8)], px_w: usize, px_h: usize) {
    let cx = px_w as f64 / 2.0;
    let cy = px_h as f64 / 2.0;
    let max_dist = (cx * cx + cy * cy).sqrt();
    for py in 0..px_h {
        for px_x in 0..px_w {
            let dx = px_x as f64 - cx;
            let dy = py as f64 - cy;
            let t = ((dx * dx + dy * dy).sqrt() / max_dist).min(1.0);
            let v = (20.0 * (1.0 - t * 0.4)) as u8;
            fb[py * px_w + px_x] = (v / 3, v / 3, v);
        }
    }
}

pub fn shadow_pass(fb: &mut [(u8, u8, u8)], hit_mask: &[bool], px_w: usize, px_h: usize) {
    let sdx = 4isize;
    let sdy = 6isize;
    for py in 0..px_h {
        for px_x in 0..px_w {
            if hit_mask[py * px_w + px_x] {
                continue;
            }
            let sx = px_x as isize - sdx;
            let sy = py as isize - sdy;
            if sx >= 0
                && (sx as usize) < px_w
                && sy >= 0
                && (sy as usize) < px_h
                && hit_mask[sy as usize * px_w + sx as usize]
            {
                let c = &mut fb[py * px_w + px_x];
                c.0 = (c.0 as f64 * 0.4) as u8;
                c.1 = (c.1 as f64 * 0.4) as u8;
                c.2 = (c.2 as f64 * 0.4) as u8;
            }
        }
    }
}

pub fn fb_to_lines(fb: &[(u8, u8, u8)], px_w: usize, px_h: usize, height: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(height);
    for row in 0..height {
        let top_y = row * 2;
        let bot_y = row * 2 + 1;
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(px_w);

        for col in 0..px_w {
            let top = fb[top_y * px_w + col];
            let bot = if bot_y < px_h {
                fb[bot_y * px_w + col]
            } else {
                (0, 0, 0)
            };

            spans.push(Span::styled(
                "▀",
                Style::default()
                    .fg(Color::Rgb(top.0, top.1, top.2))
                    .bg(Color::Rgb(bot.0, bot.1, bot.2)),
            ));
        }
        lines.push(Line::from(spans));
    }
    lines
}

pub fn normalize(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-10 {
        return [0.0, 0.0, 1.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn specular(normal: [f64; 3], light: [f64; 3], power: f64) -> f64 {
    let ndotl = dot(normal, light);
    let reflect = [
        2.0 * ndotl * normal[0] - light[0],
        2.0 * ndotl * normal[1] - light[1],
        2.0 * ndotl * normal[2] - light[2],
    ];
    let view_dot = -reflect[2];
    view_dot.max(0.0).powf(power)
}

pub fn shade(channel: u8, brightness: f64, spec: f64) -> u8 {
    ((channel as f64 * brightness + spec * 255.0).min(255.0)) as u8
}
