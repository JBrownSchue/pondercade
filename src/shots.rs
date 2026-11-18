//! Debug-only helper: `PONDER_SHOTS=<dir>` makes the app cycle through every screen,
//! save a PNG screenshot of each, then exit. Compiled out of release builds.
use eframe::egui::{self, ColorImage, ViewportCommand};
use std::path::PathBuf;

pub struct Shooter {
    dir: PathBuf,
    frame: u32,
}

impl Shooter {
    pub fn from_env() -> Option<Self> {
        std::env::var_os("PONDER_SHOTS").map(|d| Self { dir: d.into(), frame: 0 })
    }

    /// Returns (Some(screen index) when the app should switch screens, whether to run the demo).
    pub fn tick(&mut self, ctx: &egui::Context, screens: usize) -> (Option<usize>, bool) {
        const STEP: u32 = 45;
        let (idx, phase) = ((self.frame / STEP) as usize, self.frame % STEP);
        if self.frame < 40 || self.frame % 20 == 0 {
            let line = ctx.input(|i| {
                let v = i.viewport();
                format!(
                    "frame {} inner={:?} outer={:?} native_ppp={:?} monitor={:?} maximized={:?} screen_rect={:?}
",
                    self.frame, v.inner_rect, v.outer_rect, v.native_pixels_per_point, v.monitor_size, v.maximized, i.content_rect()
                )
            });
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join("log.txt")) {
                let _ = f.write_all(line.as_bytes());
            }
        }
        self.frame += 1;
        ctx.request_repaint();
        if idx >= screens {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return (None, false);
        }
        if phase == STEP - 5 {
            let path = self.dir.join(format!("{idx}.png"));
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                let _ = std::fs::write(path, png(&img));
            })));
        }
        ((phase == 0).then_some(idx), phase == 8)
    }
}

/// Minimal uncompressed PNG encoder (stored deflate blocks).
fn png(img: &ColorImage) -> Vec<u8> {
    let [w, h] = img.size;
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for y in 0..h {
        raw.push(0);
        for x in 0..w {
            let c = img.pixels[y * w + x];
            raw.extend_from_slice(&[c.r(), c.g(), c.b()]);
        }
    }
    let mut z = vec![0x78, 0x01];
    let chunks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, ch) in chunks.iter().enumerate() {
        z.push(u8::from(i + 1 == chunks.len()));
        z.extend_from_slice(&(ch.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(ch.len() as u16)).to_le_bytes());
        z.extend_from_slice(ch);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    for (ty, data) in [(b"IHDR", ihdr), (b"IDAT", z), (b"IEND", Vec::new())] {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(ty);
        out.extend_from_slice(&data);
        let crc = out[start..].iter().fold(!0u32, |mut c, &byte| {
            c ^= byte as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            c
        });
        out.extend_from_slice(&(!crc).to_be_bytes());
    }
    out
}
