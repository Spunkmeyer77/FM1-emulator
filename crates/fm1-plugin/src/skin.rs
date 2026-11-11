//! The FM-1 emulator skin (Transistor Labs): the panel artwork with knobs, keys, buttons and the
//! screen drawn over it, and the hit map from panel coordinates to the emulator's controls.
//! Coordinates are in the artwork's pixels (1536 x 1024).

use nih_plug_egui::egui::{self, Color32, Mesh, Pos2, Rect, Rot2, Shape, TextureHandle, TextureOptions, Vec2};

pub const W: usize = 1536;
pub const H: usize = 1024;

/// Knob centres in the order of the emulator's encoders (SELECT, PRESETS, ALGORITHM, KNOB1..4).
pub const ENCS: [(f32, f32); 7] = [
    (279.0, 193.0), (112.0, 381.0), (279.0, 381.0),
    (859.5, 193.0), (1038.0, 193.0), (1222.5, 193.0), (1412.5, 193.0),
];
pub const POT: (f32, f32) = (112.0, 193.0);
const KNOB_R: f32 = 52.0;
const LCD: (f32, f32, f32, f32) = (417.0, 155.0, 308.0, 308.0);

/// Buttons: (matrix id, x, y, w, h) of each cap.
fn buttons() -> Vec<(u8, f32, f32, f32, f32)> {
    let mut v = vec![(0, 75.0, 478.0, 105.0, 54.0), (1, 217.0, 478.0, 103.0, 54.0)];
    for r in 0..2 {
        for i in 0..6 {
            v.push(((2 + r * 6 + i) as u8, 831.0 + i as f32 * 109.4, 328.0 + r as f32 * 110.0, 75.0, 70.0));
        }
    }
    v
}

/// Note keys: (matrix id, x, y, w, h, black). F3..G5, as on the FM-1.
fn keys() -> Vec<(u8, f32, f32, f32, f32, bool)> {
    let (pitch, gap, x0) = (90.0, 14.0, 53.0);
    let whites = [0u8, 2, 4, 6, 7, 9, 11, 12, 14, 16, 18, 19, 21, 23, 24, 26];
    let mut v: Vec<_> = whites.iter().enumerate()
        .map(|(i, n)| (14 + n, x0 + i as f32 * pitch, 792.0, pitch - gap, 172.0, false)).collect();
    let blacks = [(1u8, 0usize), (3, 1), (5, 2), (8, 4), (10, 5), (13, 7), (15, 8), (17, 9), (20, 11), (22, 12), (25, 14)];
    for (n, after) in blacks {
        let cx = x0 + (after + 1) as f32 * pitch - gap / 2.0;
        v.push((14 + n, (cx - 33.0).round(), 604.0, 66.0, 162.0, true));
    }
    v
}

/// What a panel point is, in the editor's codes: 0..6 an encoder, 7 MASTER, 100 + id a button or key.
pub fn hit(x: f32, y: f32) -> i32 {
    for (i, (cx, cy)) in ENCS.iter().enumerate() {
        if (x - cx).powi(2) + (y - cy).powi(2) <= (KNOB_R + 8.0).powi(2) {
            return i as i32;
        }
    }
    if (x - POT.0).powi(2) + (y - POT.1).powi(2) <= (KNOB_R + 8.0).powi(2) {
        return 7;
    }
    let inside = |px: f32, py: f32, w: f32, h: f32| x >= px && x < px + w && y >= py && y < py + h;
    for (id, px, py, w, h) in buttons() {
        if inside(px, py, w, h) {
            return 100 + id as i32;
        }
    }
    let mut ks = keys();
    ks.sort_by_key(|k| !k.5); // black keys first
    for (id, px, py, w, h, _) in ks {
        if inside(px, py, w, h) {
            return 100 + id as i32;
        }
    }
    -1
}

fn decode(bytes: &[u8]) -> egui::ColorImage {
    let mut dec = png::Decoder::new(bytes);
    dec.set_transformations(png::Transformations::EXPAND);
    let mut r = dec.read_info().expect("skin png");
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).expect("skin png");
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..w * h * 4].to_vec(),
        png::ColorType::Rgb => buf[..w * h * 3].chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        _ => panic!("skin png colour type"),
    };
    egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba)
}

/// The skin's textures, made once per window.
pub struct Skin {
    panel: TextureHandle,
    knob: TextureHandle,
    lcd: TextureHandle,
    /// [white, black] x [pressed, lit, lit + pressed]: (texture, pad x, pad y)
    keys: [[(TextureHandle, f32, f32); 3]; 2],
    rgba: Vec<u8>,
}

impl Skin {
    pub fn new(ctx: &egui::Context) -> Skin {
        let t = |name: &str, bytes: &[u8]| ctx.load_texture(name, decode(bytes), TextureOptions::LINEAR);
        Skin {
            panel: t("skin_panel", include_bytes!("../skin/panel.png")),
            knob: t("skin_knob", include_bytes!("../skin/knob.png")),
            lcd: ctx.load_texture("skin_lcd", egui::ColorImage::new([240, 240], Color32::BLACK), TextureOptions::LINEAR),
            keys: [
                [
                    (t("w_p", include_bytes!("../skin/white_pressed.png")), 0.0, 0.0),
                    (t("w_l", include_bytes!("../skin/white_lit.png")), 9.0, 10.0),
                    (t("w_lp", include_bytes!("../skin/white_litp.png")), 9.0, 10.0),
                ],
                [
                    (t("b_p", include_bytes!("../skin/black_pressed.png")), 0.0, 0.0),
                    (t("b_l", include_bytes!("../skin/black_lit.png")), 8.0, 10.0),
                    (t("b_lp", include_bytes!("../skin/black_litp.png")), 8.0, 9.0),
                ],
            ],
            rgba: Vec::with_capacity(240 * 240 * 4),
        }
    }

    /// Draw the panel into `rect` (the artwork's aspect).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, painter: &egui::Painter, rect: Rect, lcd: &[u16], enc_clicks: &[i32], pot: f32,
                down: &[bool], leds: &[f32]) {
        let k = rect.width() / W as f32;
        let at = |x: f32, y: f32| rect.min + Vec2::new(x * k, y * k);
        let r = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(at(x, y), Vec2::new(w * k, h * k));
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        painter.image(self.panel.id(), rect, uv, Color32::WHITE);

        // the screen (RGB565)
        self.rgba.clear();
        for &p in lcd.iter().take(240 * 240) {
            let p = p as u32;
            self.rgba.extend([((p >> 11) * 255 / 31) as u8, (((p >> 5) & 63) * 255 / 63) as u8, ((p & 31) * 255 / 31) as u8, 255]);
        }
        if self.rgba.len() == 240 * 240 * 4 {
            self.lcd.set(egui::ColorImage::from_rgba_unmultiplied([240, 240], &self.rgba), TextureOptions::LINEAR);
        }
        painter.image(self.lcd.id(), r(LCD.0, LCD.1, LCD.2, LCD.3), uv, Color32::WHITE);

        // knobs: encoders turn 18 degrees a click; MASTER sweeps 270 degrees
        let knob = |painter: &egui::Painter, (cx, cy): (f32, f32), angle: f32| {
            let c = at(cx, cy);
            let mut m = Mesh::with_texture(self.knob.id());
            m.add_rect_with_uv(Rect::from_center_size(c, Vec2::splat(2.0 * KNOB_R * k)), uv, Color32::WHITE);
            m.rotate(Rot2::from_angle(angle), c);
            painter.add(Shape::mesh(m));
        };
        knob(painter, POT, (pot * 270.0 - 135.0).to_radians());
        for (i, &c) in ENCS.iter().enumerate() {
            knob(painter, c, (enc_clicks.get(i).copied().unwrap_or(0) as f32 * 18.0).to_radians());
        }

        let lit = |id: u8| leds.get(id as usize).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let pressed = |id: u8| down.get(id as usize).copied().unwrap_or(false);

        // buttons: pressed darkens the cap; a lit LED glows orange
        for (id, x, y, w, h) in buttons() {
            let cap = r(x, y, w, h);
            let l = lit(id);
            if l > 0.02 {
                let a = (l * 255.0) as u8;
                painter.rect_filled(cap.expand(5.0 * k), 9.0 * k, Color32::from_rgba_unmultiplied(255, 138, 31, a / 4));
                painter.rect_filled(cap, 6.0 * k, Color32::from_rgba_unmultiplied(255, 150, 40, a / 3));
            }
            if pressed(id) {
                painter.rect_filled(cap, 6.0 * k, Color32::from_black_alpha(70));
            }
        }

        // keys: the state sprites over the artwork's keys
        let mut ks = keys();
        ks.sort_by_key(|k| k.5); // white keys first, black on top
        for (id, x, y, w, h, black) in ks {
            let l = lit(id);
            let p = pressed(id);
            let set = &self.keys[black as usize];
            if l > 0.02 {
                let (tex, px, py) = &set[if p { 2 } else { 1 }];
                let tint = Color32::from_white_alpha((l * 255.0) as u8);
                painter.image(tex.id(), r(x - px, y - py, w + 2.0 * px, h + 2.0 * py), uv, tint);
            } else if p {
                painter.image(set[0].0.id(), r(x, y, w, h), uv, Color32::WHITE);
            }
        }
    }
}
