//! NyashkMeter GUI: loudness, peaks, spectrum, stereo image, history, tonal balance.
use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, epaint::PathStroke, Align2, Color32, Pos2, Rect, Stroke, Ui, Vec2},
};
use nyashk_common::fft::Fft;
use nyashk_common::ui::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use crate::*;

const BASE_W: f32 = 1080.0;
const BASE_H: f32 = 570.0;
const FFT_N: usize = 8192;
const F_LO: f32 = 20.0;
const F_HI: f32 = 20000.0;
const DB_LO: f32 = -96.0;
const DB_HI: f32 = 0.0;

struct Gui {
    fft: Fft,
    frame: Vec<f32>,
    db: Vec<f32>,
    smooth: Vec<f32>,
    hold: Vec<f32>,
    gl: Vec<f32>,
    gr: Vec<f32>,
    hm: Vec<f32>,
    hs: Vec<f32>,
    tp_l: f32,
    tp_r: f32,
    corr: f32,
    bands: [f32; 6],
}

fn lufs(v: f32) -> String {
    if v < -100.0 {
        "-inf".into()
    } else {
        format!("{:.1}", v)
    }
}

fn pill_row(ui: &mut Ui, origin: Pos2, labels: &[&str], cur: Option<usize>, w: f32, h: f32, tips: &[&str]) -> Option<usize> {
    let mut hit = None;
    for (i, l) in labels.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(origin.x + (w + 4.0) * i as f32, origin.y), Vec2::new(w, h));
        if pill_tip(ui, r, l, cur == Some(i), tips.get(i).copied().unwrap_or("")) {
            hit = Some(i);
        }
    }
    hit
}

pub fn create(params: Arc<MeterParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();
    let size_state = editor_state.clone();
    create_egui_editor(
        editor_state,
        Gui {
            fft: Fft::new(FFT_N),
            frame: vec![0.0; FFT_N],
            db: vec![-140.0; FFT_N / 2],
            smooth: vec![-140.0; FFT_N / 2],
            hold: vec![-140.0; FFT_N / 2],
            gl: vec![0.0; 1024],
            gr: vec![0.0; 1024],
            hm: Vec::new(),
            hs: Vec::new(),
            tp_l: -90.0,
            tp_r: -90.0,
            corr: 0.0,
            bands: [-120.0; 6],
        },
        |ctx, _| apply_theme(ctx),
        move |ctx, setter, st| {
            ctx.request_repaint_after(Duration::from_millis(33));
            let p = &*params;
            let fs = f32::from_bits(shared.fs.load(Ordering::Relaxed)).max(8000.0);
            let sh = &*shared;

            // ---- spectrum analysis
            let src = p.spec_src.value();
            match src {
                SpecSrc::Mix | SpecSrc::Mid => sh.tap_m.snapshot(&mut st.frame),
                SpecSrc::Side => sh.tap_s.snapshot(&mut st.frame),
                SpecSrc::Left => sh.tap_l.snapshot(&mut st.frame),
                SpecSrc::Right => sh.tap_r.snapshot(&mut st.frame),
            }
            st.fft.magnitude_db(&st.frame, &mut st.db);
            let (up, down) = match p.speed.value() {
                Speed::Fast => (0.6, 0.35),
                Speed::Medium => (0.45, 0.18),
                Speed::Slow => (0.3, 0.07),
            };
            for k in 0..FFT_N / 2 {
                let v = st.db[k];
                // average in the power domain so noise does not look jagged
                let s = &mut st.smooth[k];
                *s = if v > *s { *s + (v - *s) * up } else { *s + (v - *s) * down };
                st.hold[k] = (st.hold[k] - 0.3).max(*s);
            }
            // tonal balance (energy in six bands)
            let bin_hz = fs / FFT_N as f32;
            let edges = [20.0f32, 60.0, 250.0, 500.0, 2000.0, 6000.0, 20000.0];
            for b in 0..6 {
                let (ka, kb) = (((edges[b] / bin_hz) as usize).max(1), ((edges[b + 1] / bin_hz) as usize).min(FFT_N / 2 - 1));
                let e: f32 = (ka..=kb).map(|k| 10f32.powf(st.smooth[k] / 10.0)).sum();
                st.bands[b] += (10.0 * (e + 1e-14).log10() - st.bands[b]) * 0.15;
            }
            // goniometer + meters
            sh.tap_l.snapshot(&mut st.gl);
            sh.tap_r.snapshot(&mut st.gr);
            let fall = |cur: f32, t: f32| if t > cur { t } else { cur - 1.0 };
            st.tp_l = fall(st.tp_l, sh.get(V_TPL));
            st.tp_r = fall(st.tp_r, sh.get(V_TPR));
            st.corr += (sh.get(V_CORR) - st.corr) * 0.25;

            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(BG)).show(ctx, |ui| {
                let full = ui.max_rect();
                let o = full.min;
                let rect = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(Pos2::new(o.x + x, o.y + y), Vec2::new(w, h));
                header(ui, o, BASE_W, "Meter", "LOUDNESS / SPECTRUM ANALYZER");
                let painter = ui.painter().clone();
                let target = p.target.value().lufs();

                // ================= A: loudness
                let a = rect(14.0, 70.0, 270.0, 316.0);
                panel(ui, a);
                let integ = sh.get(V_I);
                painter.text(Pos2::new(a.center().x, a.top() + 18.0), Align2::CENTER_CENTER, "INTEGRATED", mono(10.5), DIM);
                painter.text(Pos2::new(a.center().x - 10.0, a.top() + 52.0), Align2::CENTER_CENTER, lufs(integ), mono(40.0), WHITE);
                painter.text(Pos2::new(a.center().x + 82.0, a.top() + 62.0), Align2::LEFT_CENTER, "LUFS", mono(10.5), DIM);
                if let (Some(t), true) = (target, integ > -100.0) {
                    let d = integ - t;
                    painter.text(Pos2::new(a.center().x, a.top() + 82.0), Align2::CENTER_CENTER, format!("{:+.1} LU vs target {:.0}", d, t), mono(10.5), if d.abs() <= 1.0 { WHITE } else { DIM });
                }
                let rows = [("MOMENTARY", lufs(sh.get(V_M)), "LUFS"), ("SHORT-TERM", lufs(sh.get(V_S)), "LUFS"), ("LOUDNESS RANGE", if sh.get(V_LRA) > 0.0 { format!("{:.1}", sh.get(V_LRA)) } else { "--".into() }, "LU"), ("MAX MOMENTARY", lufs(sh.get(V_MAXM)), "LUFS"), ("MAX SHORT-TERM", lufs(sh.get(V_MAXS)), "LUFS")];
                for (i, (name, val, unit)) in rows.iter().enumerate() {
                    let y = a.top() + 108.0 + 25.0 * i as f32;
                    painter.text(Pos2::new(a.left() + 14.0, y), Align2::LEFT_CENTER, *name, mono(10.0), DIM);
                    painter.text(Pos2::new(a.right() - 48.0, y), Align2::RIGHT_CENTER, val, mono(14.0), WHITE);
                    painter.text(Pos2::new(a.right() - 14.0, y), Align2::RIGHT_CENTER, *unit, mono(9.0), DIM);
                }
                painter.hline(a.left() + 10.0..=a.right() - 10.0, a.top() + 236.0, Stroke::new(1.0, LINE));
                painter.text(Pos2::new(a.left() + 14.0, a.top() + 248.0), Align2::LEFT_CENTER, "TARGET", mono(9.5), DIM);
                let tcur = [Target::Off, Target::Loud, Target::Stream, Target::Podcast, Target::Ebu].iter().position(|t| *t == p.target.value());
                let tl = ["OFF", "-9 LOUD", "-14 STREAM"];
                let tips1 = ["No target.", "-9 LUFS: loud masters, club and genre releases.", "-14 LUFS: Spotify, YouTube, Tidal and most streaming services normalize to about this."];
                if let Some(i) = pill_row(ui, Pos2::new(a.left() + 10.0, a.top() + 258.0), &tl, tcur, 80.0, 22.0, &tips1) {
                    set_e(setter, &p.target, [Target::Off, Target::Loud, Target::Stream][i]);
                }
                let tl2 = ["-16 PODCAST", "-23 EBU"];
                let tips2 = ["-16 LUFS: Apple Music, Apple Podcasts and spoken word.", "-23 LUFS: EBU R128 broadcast standard (TV, radio in Europe)."];
                if let Some(i) = pill_row(ui, Pos2::new(a.left() + 10.0, a.top() + 284.0), &tl2, tcur.and_then(|t| t.checked_sub(3)), 80.0, 22.0, &tips2) {
                    set_e(setter, &p.target, [Target::Podcast, Target::Ebu][i]);
                }
                if pill_tip(ui, Rect::from_min_size(Pos2::new(a.left() + 10.0 + 2.0 * 84.0, a.top() + 284.0), Vec2::new(80.0, 22.0)), "RESET", false, "Clear integrated loudness, LRA, maxima and peak statistics, and start measuring again.") {
                    sh.reset.store(true, Ordering::Relaxed);
                }

                // ================= B: peaks
                let b = rect(14.0, 396.0, 270.0, 164.0);
                panel(ui, b);
                let tpm = sh.get(V_TP);
                painter.text(Pos2::new(b.left() + 14.0, b.top() + 16.0), Align2::LEFT_CENTER, "TRUE PEAK MAX", mono(10.0), DIM);
                painter.text(Pos2::new(b.left() + 14.0, b.top() + 40.0), Align2::LEFT_CENTER, if tpm < -100.0 { "-inf".into() } else { format!("{:+.1} dBTP", tpm) }, mono(22.0), if tpm > -1.0 { WHITE } else { Color32::from_rgb(0xc8, 0xc8, 0xc8) });
                let pk = sh.get(V_PEAK);
                let plr = if integ > -100.0 { tpm - integ } else { f32::NAN };
                let kv = |y: f32, k: &str, v: String| {
                    painter.text(Pos2::new(b.left() + 14.0, b.top() + y), Align2::LEFT_CENTER, k, mono(9.5), DIM);
                    painter.text(Pos2::new(b.right() - 14.0, b.top() + y), Align2::RIGHT_CENTER, v, mono(12.0), WHITE);
                };
                kv(66.0, "SAMPLE PEAK", if pk < -100.0 { "-inf".into() } else { format!("{:+.1} dBFS", pk) });
                kv(84.0, "PLR (peak/loudness)", if plr.is_nan() { "--".into() } else { format!("{:.1} dB", plr) });
                kv(102.0, "OVERS > -1 dBTP", format!("{}", sh.get(V_OVER).max(0.0) as u32));
                for (k, (name, v)) in [("L", st.tp_l), ("R", st.tp_r)].into_iter().enumerate() {
                    let r = Rect::from_min_size(Pos2::new(b.left() + 34.0, b.top() + 120.0 + 18.0 * k as f32), Vec2::new(b.width() - 48.0, 11.0));
                    painter.text(Pos2::new(b.left() + 14.0, r.center().y), Align2::LEFT_CENTER, name, mono(9.5), DIM);
                    painter.rect_filled(r, 2.0, Color32::from_rgb(7, 7, 7));
                    let t = ((v + 48.0) / 48.0).clamp(0.0, 1.0);
                    painter.rect_filled(Rect::from_min_size(r.min, Vec2::new(r.width() * t, r.height())), 2.0, if v > -1.0 { WHITE } else { Color32::from_rgb(0xb4, 0xb4, 0xb4) });
                    painter.rect_stroke(r, 2.0, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
                }

                // ================= C: spectrum
                let c = rect(294.0, 70.0, 550.0, 316.0);
                panel(ui, c);
                let g = Rect::from_min_max(Pos2::new(c.left() + 36.0, c.top() + 12.0), Pos2::new(c.right() - 12.0, c.bottom() - 76.0));
                painter.rect_filled(g, 4.0, Color32::from_rgb(7, 7, 7));
                let fx = |f: f32| g.left() + g.width() * (f / F_LO).ln() / (F_HI / F_LO).ln();
                let dy = |d: f32| g.bottom() - g.height() * ((d - DB_LO) / (DB_HI - DB_LO)).clamp(0.0, 1.0);
                for d in [-12.0f32, -24.0, -36.0, -48.0, -60.0, -72.0, -84.0] {
                    painter.hline(g.left()..=g.right(), dy(d), Stroke::new(1.0, LINE));
                    painter.text(Pos2::new(g.left() - 5.0, dy(d)), Align2::RIGHT_CENTER, format!("{:.0}", d), mono(9.0), DIM);
                }
                for f in [20.0f32, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0] {
                    painter.vline(fx(f), g.top()..=g.bottom(), Stroke::new(1.0, LINE));
                    painter.text(Pos2::new(fx(f), g.bottom() + 10.0), Align2::CENTER_CENTER, if f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{}", f) }, mono(9.0), DIM);
                }
                let slope = match p.slope.value() {
                    Slope::Flat => 0.0,
                    Slope::Three => 3.0,
                    Slope::FourHalf => 4.5,
                };
                // value of an array at a frequency range, power-averaged
                let at = |arr: &[f32], fa: f32, fb: f32| -> f32 {
                    let (ka, kb) = (fa / bin_hz, fb / bin_hz);
                    if kb - ka < 1.0 {
                        let kc = (0.5 * (ka + kb)).clamp(1.0, (FFT_N / 2 - 2) as f32);
                        let i = kc.floor() as usize;
                        let t = kc - i as f32;
                        arr[i] * (1.0 - t) + arr[i + 1] * t
                    } else {
                        let (ka, kb) = ((ka.ceil() as usize).max(1), (kb.floor() as usize).min(FFT_N / 2 - 1));
                        let e: f32 = (ka..=kb).map(|k| 10f32.powf(arr[k] / 10.0)).sum::<f32>() / (kb - ka + 1) as f32;
                        10.0 * (e + 1e-14).log10()
                    }
                };
                let clip = painter.with_clip_rect(g);
                match p.spec_mode.value() {
                    SpecMode::Curve => {
                        let cols = (g.width() / 2.0) as usize;
                        let mut line = Vec::with_capacity(cols);
                        let mut peak = Vec::with_capacity(cols);
                        for i in 0..cols {
                            let (xa, xb) = (g.left() + i as f32 * 2.0, g.left() + i as f32 * 2.0 + 2.0);
                            let to_f = |x: f32| F_LO * (F_HI / F_LO).powf((x - g.left()) / g.width());
                            let (fa, fb) = (to_f(xa), to_f(xb));
                            let fc = (fa * fb).sqrt();
                            let adj = slope * (fc / 1000.0).log2();
                            line.push(Pos2::new(xa + 1.0, dy(at(&st.smooth, fa, fb) + adj)));
                            peak.push(Pos2::new(xa + 1.0, dy(at(&st.hold, fa, fb) + adj)));
                        }
                        fill_under(&clip, &line, g.bottom(), Color32::from_white_alpha(26));
                        if p.hold.value() {
                            clip.add(egui::Shape::line(peak, PathStroke::new(1.0, Color32::from_white_alpha(80))));
                        }
                        clip.add(egui::Shape::line(line, PathStroke::new(1.8, WHITE)));
                    }
                    SpecMode::Third => {
                        for n in -17..=13 {
                            let fc = 1000.0 * 2f32.powf(n as f32 / 3.0);
                            let (fa, fb) = (fc / 2f32.powf(1.0 / 6.0), fc * 2f32.powf(1.0 / 6.0));
                            let (ka, kb) = (((fa / bin_hz).ceil() as usize).max(1), ((fb / bin_hz).floor() as usize).min(FFT_N / 2 - 1));
                            let energy = |arr: &[f32]| -> f32 {
                                if kb < ka {
                                    return at(arr, fa, fb);
                                }
                                10.0 * ((ka..=kb).map(|k| 10f32.powf(arr[k] / 10.0)).sum::<f32>() + 1e-14).log10()
                            };
                            let adj = (slope - 3.0) * (fc / 1000.0).log2();
                            let (x0, x1) = (fx(fa) + 1.0, fx(fb) - 1.0);
                            let top = dy(energy(&st.smooth) + adj);
                            clip.rect_filled(Rect::from_min_max(Pos2::new(x0, top), Pos2::new(x1, g.bottom())), 1.0, Color32::from_rgb(0xc8, 0xc8, 0xc8));
                            if p.hold.value() {
                                let ph = dy(energy(&st.hold) + adj);
                                clip.hline(x0..=x1, ph, Stroke::new(1.5, WHITE));
                            }
                        }
                    }
                }
                painter.text(Pos2::new(g.left() + 8.0, g.top() + 10.0), Align2::LEFT_CENTER, format!("SPECTRUM  ·  {}", ["L+R", "MID", "SIDE", "LEFT", "RIGHT"][p.spec_src.value().to_index()]), mono(9.0), DIM);
                // controls
                let cy = c.bottom() - 46.0;
                let lbl = |x: f32, y: f32, t: &str| painter.text(Pos2::new(c.left() + x, y), Align2::LEFT_CENTER, t, mono(9.0), DIM);
                lbl(12.0, cy + 11.0, "VIEW");
                if let Some(i) = pill_row(ui, Pos2::new(c.left() + 50.0, cy), &["CURVE", "1/3 OCT"], Some(p.spec_mode.value().to_index()), 62.0, 22.0, &["Smooth analyzer curve.", "Third-octave bars (RTA), like a hardware analyzer."]) {
                    set_e(setter, &p.spec_mode, SpecMode::from_index(i));
                }
                lbl(206.0, cy + 11.0, "SOURCE");
                if let Some(i) = pill_row(ui, Pos2::new(c.left() + 260.0, cy), &["L+R", "MID", "SIDE", "L", "R"], Some(p.spec_src.value().to_index()), 46.0, 22.0, &["Mono sum of both channels.", "Centre (mid) only.", "Sides only: what is wide in the mix.", "Left channel.", "Right channel."]) {
                    set_e(setter, &p.spec_src, SpecSrc::from_index(i));
                }
                let cy2 = cy + 26.0;
                lbl(12.0, cy2 + 11.0, "SLOPE");
                if let Some(i) = pill_row(ui, Pos2::new(c.left() + 50.0, cy2), &["0 dB", "3 dB", "4.5 dB"], Some(p.slope.value().to_index()), 52.0, 22.0, &["No tilt: raw spectrum (pink noise falls 3 dB per octave).", "3 dB/oct tilt: pink noise looks flat.", "4.5 dB/oct tilt: typical music looks flat (default)."]) {
                    set_e(setter, &p.slope, Slope::from_index(i));
                }
                lbl(222.0, cy2 + 11.0, "SPEED");
                if let Some(i) = pill_row(ui, Pos2::new(c.left() + 270.0, cy2), &["FAST", "MED", "SLOW"], Some(p.speed.value().to_index()), 50.0, 22.0, &["Quick reaction.", "Balanced.", "Calm, averaged picture."]) {
                    set_e(setter, &p.speed, Speed::from_index(i));
                }
                let hold = p.hold.value();
                if pill_tip(ui, Rect::from_min_size(Pos2::new(c.right() - 74.0, cy2), Vec2::new(62.0, 22.0)), "HOLD", hold, "Show the peak-hold line.") {
                    set_b(setter, &p.hold, !hold);
                }

                // ================= D: stereo image
                let d = rect(854.0, 70.0, 212.0, 316.0);
                panel(ui, d);
                let gs = 188.0;
                let gr = Rect::from_min_size(Pos2::new(d.left() + 12.0, d.top() + 12.0), Vec2::splat(gs));
                painter.rect_filled(gr, 4.0, Color32::from_rgb(7, 7, 7));
                let gc = gr.center();
                let rad = gs * 0.46;
                painter.line_segment([Pos2::new(gc.x, gr.top() + 4.0), Pos2::new(gc.x, gr.bottom() - 4.0)], Stroke::new(1.0, LINE));
                painter.line_segment([Pos2::new(gr.left() + 4.0, gc.y), Pos2::new(gr.right() - 4.0, gc.y)], Stroke::new(1.0, LINE));
                painter.line_segment([Pos2::new(gc.x - rad, gc.y + rad), Pos2::new(gc.x + rad, gc.y - rad)], Stroke::new(1.0, Color32::from_rgb(0x24, 0x24, 0x24)));
                painter.line_segment([Pos2::new(gc.x - rad, gc.y - rad), Pos2::new(gc.x + rad, gc.y + rad)], Stroke::new(1.0, Color32::from_rgb(0x24, 0x24, 0x24)));
                painter.text(Pos2::new(gc.x, gr.top() + 9.0), Align2::CENTER_CENTER, "M", mono(9.0), DIM);
                painter.text(Pos2::new(gr.left() + 9.0, gr.top() + 9.0), Align2::CENTER_CENTER, "L", mono(9.0), DIM);
                painter.text(Pos2::new(gr.right() - 9.0, gr.top() + 9.0), Align2::CENTER_CENTER, "R", mono(9.0), DIM);
                let gclip = painter.with_clip_rect(gr);
                let scale = 1.0 / std::f32::consts::SQRT_2;
                for i in 0..st.gl.len() {
                    let (l, r) = (st.gl[i], st.gr[i]);
                    let (side, mid) = ((r - l) * scale, (l + r) * scale);
                    let pt = Pos2::new(gc.x + side * rad * 1.35, gc.y - mid * rad * 1.35);
                    gclip.circle_filled(pt, 1.0, Color32::from_white_alpha(if i > st.gl.len() - 200 { 150 } else { 60 }));
                }
                // correlation bar
                let cr = Rect::from_min_size(Pos2::new(d.left() + 14.0, d.top() + 226.0), Vec2::new(gs - 4.0, 12.0));
                painter.text(Pos2::new(d.left() + 14.0, cr.top() - 9.0), Align2::LEFT_CENTER, "CORRELATION", mono(9.0), DIM);
                painter.text(Pos2::new(cr.right(), cr.top() - 9.0), Align2::RIGHT_CENTER, format!("{:+.2}", st.corr), mono(10.5), WHITE);
                painter.rect_filled(cr, 2.0, Color32::from_rgb(7, 7, 7));
                painter.vline(cr.center().x, cr.top()..=cr.bottom(), Stroke::new(1.0, MID));
                let cx = cr.center().x + st.corr * cr.width() * 0.5;
                painter.rect_filled(Rect::from_min_max(Pos2::new(cr.center().x.min(cx), cr.top() + 1.0), Pos2::new(cr.center().x.max(cx), cr.bottom() - 1.0)), 1.0, if st.corr < 0.0 { WHITE } else { Color32::from_rgb(0xb4, 0xb4, 0xb4) });
                painter.rect_stroke(cr, 2.0, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
                painter.text(Pos2::new(cr.left() + 2.0, cr.bottom() + 9.0), Align2::LEFT_CENTER, "-1 out of phase", mono(8.5), DIM);
                painter.text(Pos2::new(cr.right() - 2.0, cr.bottom() + 9.0), Align2::RIGHT_CENTER, "+1 mono", mono(8.5), DIM);
                // balance
                let bal = sh.get(V_RMSL) - sh.get(V_RMSR);
                painter.text(Pos2::new(d.left() + 14.0, d.top() + 280.0), Align2::LEFT_CENTER, "BALANCE", mono(9.0), DIM);
                let bt = if sh.get(V_RMSL) < -100.0 { "--".to_string() } else if bal.abs() < 0.3 { "centred".to_string() } else if bal > 0.0 { format!("L +{:.1} dB", bal) } else { format!("R +{:.1} dB", -bal) };
                painter.text(Pos2::new(cr.right(), d.top() + 280.0), Align2::RIGHT_CENTER, bt, mono(10.5), WHITE);
                let br = Rect::from_min_size(Pos2::new(d.left() + 14.0, d.top() + 292.0), Vec2::new(gs - 4.0, 8.0));
                painter.rect_filled(br, 2.0, Color32::from_rgb(7, 7, 7));
                painter.vline(br.center().x, br.top()..=br.bottom(), Stroke::new(1.0, MID));
                let bx = br.center().x - (bal / 12.0).clamp(-1.0, 1.0) * br.width() * 0.5;
                painter.circle_filled(Pos2::new(bx, br.center().y), 4.0, WHITE);

                // ================= E: loudness history
                let e = rect(294.0, 396.0, 550.0, 164.0);
                panel(ui, e);
                let eg = Rect::from_min_max(Pos2::new(e.left() + 36.0, e.top() + 12.0), Pos2::new(e.right() - 12.0, e.bottom() - 30.0));
                painter.rect_filled(eg, 4.0, Color32::from_rgb(7, 7, 7));
                let n_pts = match p.span.value() {
                    Span::Min1 => 600,
                    Span::Min5 => HISTORY,
                };
                sh.history(n_pts, &mut st.hm, &mut st.hs);
                let (l_lo, l_hi) = (-50.0f32, 0.0f32);
                let ly = |v: f32| eg.bottom() - eg.height() * ((v - l_lo) / (l_hi - l_lo)).clamp(0.0, 1.0);
                for v in [-10.0f32, -20.0, -30.0, -40.0] {
                    painter.hline(eg.left()..=eg.right(), ly(v), Stroke::new(1.0, LINE));
                    painter.text(Pos2::new(eg.left() - 5.0, ly(v)), Align2::RIGHT_CENTER, format!("{:.0}", v), mono(9.0), DIM);
                }
                if let Some(t) = target {
                    let y = ly(t);
                    let mut x = eg.left();
                    while x < eg.right() {
                        painter.hline(x..=(x + 6.0).min(eg.right()), y, Stroke::new(1.2, Color32::from_white_alpha(120)));
                        x += 12.0;
                    }
                    painter.text(Pos2::new(eg.right() - 4.0, y - 8.0), Align2::RIGHT_CENTER, format!("target {:.0}", t), mono(9.0), DIM);
                }
                let have = sh.published().min(n_pts);
                let pts_for = |arr: &Vec<f32>| -> Vec<Pos2> {
                    let n = arr.len();
                    let mut v = Vec::with_capacity(n);
                    for (i, val) in arr.iter().enumerate() {
                        if n - i > have || *val < -100.0 {
                            continue;
                        }
                        v.push(Pos2::new(eg.left() + eg.width() * i as f32 / (n.max(2) - 1) as f32, ly(*val)));
                    }
                    v
                };
                let eclip = painter.with_clip_rect(eg);
                let (mp, sp) = (pts_for(&st.hm), pts_for(&st.hs));
                if mp.len() > 1 {
                    eclip.add(egui::Shape::line(mp, PathStroke::new(1.0, Color32::from_white_alpha(90))));
                }
                if sp.len() > 1 {
                    eclip.add(egui::Shape::line(sp, PathStroke::new(2.0, WHITE)));
                }
                painter.text(Pos2::new(eg.left() + 8.0, eg.top() + 10.0), Align2::LEFT_CENTER, "LOUDNESS HISTORY  ·  thin = momentary  ·  bold = short-term  (LUFS)", mono(9.0), DIM);
                if let Some(i) = pill_row(ui, Pos2::new(e.right() - 12.0 - 2.0 * 64.0 - 4.0, e.bottom() - 24.0), &["1 MIN", "5 MIN"], Some(p.span.value().to_index()), 64.0, 20.0, &["Show the last minute.", "Show the last five minutes."]) {
                    set_e(setter, &p.span, Span::from_index(i));
                }
                painter.text(Pos2::new(e.left() + 12.0, e.bottom() - 14.0), Align2::LEFT_CENTER, "now →", mono(9.0), DIM);

                // ================= F: tonal balance
                let f = rect(854.0, 396.0, 212.0, 164.0);
                panel(ui, f);
                painter.text(Pos2::new(f.left() + 14.0, f.top() + 16.0), Align2::LEFT_CENTER, "TONAL BALANCE", mono(10.0), DIM);
                let names = ["SUB", "BASS", "LOW MID", "MID", "HI MID", "HIGH"];
                let top = st.bands.iter().cloned().fold(-200.0f32, f32::max);
                for (i, n) in names.iter().enumerate() {
                    let y = f.top() + 40.0 + 20.0 * i as f32;
                    painter.text(Pos2::new(f.left() + 14.0, y), Align2::LEFT_CENTER, *n, mono(9.5), DIM);
                    let r = Rect::from_min_size(Pos2::new(f.left() + 72.0, y - 5.0), Vec2::new(f.width() - 88.0, 10.0));
                    painter.rect_filled(r, 2.0, Color32::from_rgb(7, 7, 7));
                    let t = ((st.bands[i] - top + 30.0) / 30.0).clamp(0.0, 1.0);
                    if top > -100.0 {
                        painter.rect_filled(Rect::from_min_size(r.min, Vec2::new(r.width() * t, r.height())), 2.0, Color32::from_rgb(0xc8, 0xc8, 0xc8));
                    }
                    painter.rect_stroke(r, 2.0, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
                }

                resize_grip(ui, ctx, &size_state, full, BASE_W, BASE_H, 760.0);
            });
        },
    )
}
