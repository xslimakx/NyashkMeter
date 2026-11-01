//! Developer helper for screenshots: with `NYASHK_DEMO=1` the standalone builds replace the input
//! with a music-like test signal (bass, mids, a bit of top, decorrelated noise).
use nih_plug::prelude::Buffer;
use std::sync::Mutex;

struct State {
    n: u64,
    rng: u32,
    lp: [f32; 2],
    on: bool,
}
static ST: Mutex<Option<State>> = Mutex::new(None);

pub fn fill(buffer: &mut Buffer, fs: f32) {
    let mut g = ST.lock().unwrap();
    let st = g.get_or_insert_with(|| State { n: 0, rng: 12345, lp: [0.0; 2], on: std::env::var("NYASHK_DEMO").is_ok() });
    if !st.on {
        return;
    }
    for mut frame in buffer.iter_samples() {
        let t = st.n as f32 / fs;
        st.n += 1;
        let beat = (-(t * 2.0 % 1.0) * 5.0).exp();
        let mut v = [0.0f32; 2];
        for (c, s) in frame.iter_mut().enumerate() {
            st.rng = st.rng.wrapping_mul(1664525).wrapping_add(1013904223);
            let white = (st.rng >> 8) as f32 / 8388608.0 - 1.0;
            st.lp[c] += 0.15 * (white - st.lp[c]);
            let w = std::f32::consts::TAU * t;
            let x = 0.35 * beat * (w * 55.0).sin()
                + 0.12 * (w * 220.0).sin()
                + 0.08 * (w * 880.0 + c as f32).sin()
                + 0.04 * (w * 3300.0).sin()
                + 0.10 * st.lp[c]
                + 0.02 * white;
            v[c] = x;
            *s = x * 0.9;
        }
        let _ = v;
    }
}

/// Replace NaN / infinite input samples by silence, so one bad sample from a host cannot
/// poison the filter states of a plugin forever.
pub fn sanitize(buffer: &mut Buffer) {
    for ch in buffer.as_slice() {
        for s in ch.iter_mut() {
            if !s.is_finite() {
                *s = 0.0;
            }
        }
    }
}
