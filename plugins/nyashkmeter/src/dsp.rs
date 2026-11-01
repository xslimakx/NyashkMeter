//! Loudness (ITU-R BS.1770-4 / EBU R128), true peak, correlation: the measuring engine.
use nyashk_common::dsp::*;

const H_LO: f32 = -70.0;
const H_RES: f32 = 0.1; // LU per histogram bin
const H_BINS: usize = 900; // -70 .. +20 LUFS

#[inline]
fn lufs_of(ms: f32) -> f32 {
    -0.691 + 10.0 * (ms + 1e-12).log10()
}

/// K-weighting filters (ITU-R BS.1770-4), valid for any sample rate.
pub fn k_filters(fs: f32) -> [Coef; 2] {
    let fs = fs as f64;
    // stage 1: high shelf (head model)
    let (f0, g, q) = (1681.974450955533_f64, 3.999843853973347_f64, 0.7071752369554196_f64);
    let k = (std::f64::consts::PI * f0 / fs).tan();
    let vh = 10f64.powf(g / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Coef {
        b0: ((vh + vb * k / q + k * k) / a0) as f32,
        b1: (2.0 * (k * k - vh) / a0) as f32,
        b2: ((vh - vb * k / q + k * k) / a0) as f32,
        a1: (2.0 * (k * k - 1.0) / a0) as f32,
        a2: ((1.0 - k / q + k * k) / a0) as f32,
    };
    // stage 2: RLB high-pass
    let (f0, q) = (38.13547087602444_f64, 0.5003270373238773_f64);
    let k = (std::f64::consts::PI * f0 / fs).tan();
    let a0 = 1.0 + k / q + k * k;
    let hp = Coef { b0: 1.0, b1: -2.0, b2: 1.0, a1: (2.0 * (k * k - 1.0) / a0) as f32, a2: ((1.0 - k / q + k * k) / a0) as f32 };
    [shelf, hp]
}

/// 4x oversampling true-peak detector (16-tap windowed sinc, <0.1 dB error up to 0.45 fs).
#[derive(Clone)]
struct TruePeak {
    hist: [f32; 16],
    h: [[f32; 16]; 3],
}
impl TruePeak {
    fn new() -> Self {
        let mut h = [[0.0f32; 16]; 3];
        for (p, row) in h.iter_mut().enumerate() {
            let t = 7.0 + (p as f32 + 1.0) / 4.0;
            for (i, c) in row.iter_mut().enumerate() {
                let d = t - i as f32;
                let sinc = if d.abs() < 1e-6 { 1.0 } else { (std::f32::consts::PI * d).sin() / (std::f32::consts::PI * d) };
                let w = if d.abs() < 8.0 { 0.5 + 0.5 * (std::f32::consts::PI * d / 8.0).cos() } else { 0.0 };
                *c = sinc * w;
            }
        }
        Self { hist: [0.0; 16], h }
    }
    /// Push a sample, return the largest absolute value among the 4 interpolated points (delayed by 8 samples).
    #[inline]
    fn push(&mut self, x: f32) -> f32 {
        self.hist.copy_within(1..16, 0);
        self.hist[15] = x;
        let mut pk = self.hist[7].abs();
        for row in &self.h {
            let mut y = 0.0;
            for i in 0..16 {
                y += row[i] * self.hist[i];
            }
            pk = pk.max(y.abs());
        }
        pk
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Readout {
    pub momentary: f32,
    pub short_term: f32,
    pub integrated: f32,
    pub lra: f32,
    pub max_momentary: f32,
    pub max_short: f32,
    pub tp_max: f32, // dBTP, both channels
    pub tp_l: f32,   // current block, dBTP
    pub tp_r: f32,
    pub peak_max: f32, // dBFS sample peak
    pub rms_l: f32,
    pub rms_r: f32,
    pub corr: f32,
    pub tp_over: u32, // count of true-peak values above -1 dBTP
}

pub struct Meter {
    fs: f32,
    kc: [Coef; 2],
    kst: [[Bq; 2]; 2],
    tp: [TruePeak; 2],
    sub_n: usize,
    sub_len: usize,
    sub_e: f32,
    ring: [f32; 30],
    ring_n: usize,
    ring_pos: usize,
    hist_ms: Vec<f32>,   // energy sum per bin (integrated)
    hist_cnt: Vec<u32>,  // count per bin (integrated)
    lra_cnt: Vec<u32>,   // count per bin (loudness range)
    pub out: Readout,
    pub new_blocks: Vec<(f32, f32)>,
    tp_block: [f32; 2],
    peak_all: f32,
    tp_all: f32,
    sq: [f32; 2],
    corr_lr: f32,
    corr_ll: f32,
    corr_rr: f32,
    corr_a: f32,
    over_state: bool,
    pub max_m_lin: f32,
}

impl Meter {
    pub fn new(fs: f32) -> Self {
        Self {
            fs,
            kc: k_filters(fs),
            kst: Default::default(),
            tp: [TruePeak::new(), TruePeak::new()],
            sub_n: 0,
            sub_len: (fs * 0.1).round() as usize,
            sub_e: 0.0,
            ring: [0.0; 30],
            ring_n: 0,
            ring_pos: 0,
            hist_ms: vec![0.0; H_BINS],
            hist_cnt: vec![0; H_BINS],
            lra_cnt: vec![0; H_BINS],
            out: Readout { momentary: -120.0, short_term: -120.0, integrated: -120.0, max_momentary: -120.0, max_short: -120.0, tp_max: -120.0, tp_l: -120.0, tp_r: -120.0, peak_max: -120.0, rms_l: -120.0, rms_r: -120.0, corr: 0.0, lra: 0.0, tp_over: 0 },
            new_blocks: Vec::with_capacity(256),
            tp_block: [0.0; 2],
            peak_all: 0.0,
            tp_all: 0.0,
            sq: [0.0; 2],
            corr_lr: 0.0,
            corr_ll: 0.0,
            corr_rr: 0.0,
            corr_a: time_coef(300.0, fs),
            over_state: false,
            max_m_lin: 0.0,
        }
    }

    /// Forget everything measured so far (integrated, LRA, maxima, true-peak count).
    pub fn reset_stats(&mut self) {
        self.hist_ms.iter_mut().for_each(|v| *v = 0.0);
        self.hist_cnt.iter_mut().for_each(|v| *v = 0);
        self.lra_cnt.iter_mut().for_each(|v| *v = 0);
        self.ring = [0.0; 30];
        self.ring_n = 0;
        self.ring_pos = 0;
        self.peak_all = 0.0;
        self.tp_all = 0.0;
        self.out.integrated = -120.0;
        self.out.lra = 0.0;
        self.out.max_momentary = -120.0;
        self.out.max_short = -120.0;
        self.out.tp_max = -120.0;
        self.out.peak_max = -120.0;
        self.out.tp_over = 0;
    }

    pub fn reset_filters(&mut self) {
        self.kst = Default::default();
        self.tp = [TruePeak::new(), TruePeak::new()];
        self.sub_n = 0;
        self.sub_e = 0.0;
    }

    pub fn fs(&self) -> f32 {
        self.fs
    }

    /// One audio frame. `n` = 1 (mono) or 2.
    #[inline]
    pub fn frame(&mut self, x: &[f32; 2], n: usize) {
        let mut e = 0.0;
        for i in 0..n {
            let s = x[i];
            let y0 = self.kst[i][0].tick(&self.kc[0], s);
            let y = self.kst[i][1].tick(&self.kc[1], y0);
            e += y * y;
            self.peak_all = self.peak_all.max(s.abs());
            let t = self.tp[i].push(s);
            self.tp_block[i] = self.tp_block[i].max(t);
            self.sq[i] = self.sq[i] * 0.9998 + s * s * 0.0002;
        }
        self.sub_e += e;
        let (l, r) = (x[0], x[n - 1]);
        self.corr_lr = self.corr_lr * self.corr_a + l * r * (1.0 - self.corr_a);
        self.corr_ll = self.corr_ll * self.corr_a + l * l * (1.0 - self.corr_a);
        self.corr_rr = self.corr_rr * self.corr_a + r * r * (1.0 - self.corr_a);
        self.sub_n += 1;
        if self.sub_n >= self.sub_len {
            self.end_sub_block();
        }
    }

    fn bin(l: f32) -> Option<usize> {
        if l < H_LO {
            return None;
        }
        let b = ((l - H_LO) / H_RES) as usize;
        Some(b.min(H_BINS - 1))
    }

    fn end_sub_block(&mut self) {
        let ms = self.sub_e / self.sub_n as f32;
        self.sub_n = 0;
        self.sub_e = 0.0;
        self.ring[self.ring_pos] = ms;
        self.ring_pos = (self.ring_pos + 1) % 30;
        self.ring_n = (self.ring_n + 1).min(30);
        let mean_of = |ring: &[f32; 30], pos: usize, count: usize| -> f32 {
            let mut s = 0.0;
            for k in 0..count {
                s += ring[(pos + 30 - 1 - k) % 30];
            }
            s / count as f32
        };
        let o = &mut self.out;
        if self.ring_n >= 4 {
            let m = mean_of(&self.ring, self.ring_pos, 4);
            o.momentary = lufs_of(m);
            o.max_momentary = o.max_momentary.max(o.momentary);
            // gated block for the integrated loudness
            if let Some(b) = Self::bin(o.momentary) {
                self.hist_cnt[b] += 1;
                self.hist_ms[b] += m;
            }
        } else {
            o.momentary = lufs_of(mean_of(&self.ring, self.ring_pos, self.ring_n));
        }
        let cnt = self.ring_n.min(30);
        let s = mean_of(&self.ring, self.ring_pos, cnt);
        o.short_term = lufs_of(s);
        if self.ring_n >= 30 {
            o.max_short = o.max_short.max(o.short_term);
            if let Some(b) = Self::bin(o.short_term) {
                self.lra_cnt[b] += 1;
            }
        }
        // integrated: absolute gate -70 (done by the histogram range), relative gate -10 LU
        let (mut sum, mut n) = (0.0f64, 0u64);
        for b in 0..H_BINS {
            sum += self.hist_ms[b] as f64;
            n += self.hist_cnt[b] as u64;
        }
        if n > 0 {
            let gate = lufs_of((sum / n as f64) as f32) - 10.0;
            let (mut s2, mut n2) = (0.0f64, 0u64);
            for b in 0..H_BINS {
                if H_LO + b as f32 * H_RES >= gate {
                    s2 += self.hist_ms[b] as f64;
                    n2 += self.hist_cnt[b] as u64;
                }
            }
            if n2 > 0 {
                o.integrated = lufs_of((s2 / n2 as f64) as f32);
            }
        }
        // loudness range (EBU Tech 3342): gate -70 abs, -20 LU relative, 10th..95th percentile
        let total: u64 = self.lra_cnt.iter().map(|&c| c as u64).sum();
        if total > 0 {
            // mean energy of the abs-gated short-term values, from bin centres
            let mut e = 0.0f64;
            for b in 0..H_BINS {
                let l = H_LO + (b as f32 + 0.5) * H_RES;
                e += self.lra_cnt[b] as f64 * 10f64.powf(((l + 0.691) / 10.0) as f64);
            }
            let rel = lufs_of((e / total as f64) as f32) - 20.0;
            let first = Self::bin(rel.max(H_LO)).unwrap_or(0);
            let kept: u64 = self.lra_cnt[first..].iter().map(|&c| c as u64).sum();
            if kept > 1 {
                let pct = |p: f32| -> f32 {
                    let target = (p * (kept - 1) as f32).round() as u64;
                    let mut acc = 0u64;
                    for b in first..H_BINS {
                        acc += self.lra_cnt[b] as u64;
                        if acc > target {
                            return H_LO + (b as f32 + 0.5) * H_RES;
                        }
                    }
                    H_LO + H_BINS as f32 * H_RES
                };
                o.lra = (pct(0.95) - pct(0.10)).max(0.0);
            }
        }
        // peaks and stereo
        let to_db = |v: f32| 20.0 * (v + 1e-9).log10();
        o.tp_l = to_db(self.tp_block[0]);
        o.tp_r = to_db(self.tp_block[1].max(if self.tp_block[1] == 0.0 { self.tp_block[0] } else { 0.0 }));
        let blk = self.tp_block[0].max(self.tp_block[1]);
        self.tp_all = self.tp_all.max(blk);
        o.tp_max = to_db(self.tp_all);
        o.peak_max = to_db(self.peak_all);
        let over = to_db(blk) > -1.0;
        if over && !self.over_state {
            o.tp_over += 1;
        }
        self.over_state = over;
        self.tp_block = [0.0; 2];
        o.rms_l = 10.0 * (self.sq[0] + 1e-12).log10();
        o.rms_r = 10.0 * (self.sq[1] + 1e-12).log10();
        let den = (self.corr_ll * self.corr_rr).sqrt();
        o.corr = if den > 1e-9 { (self.corr_lr / den).clamp(-1.0, 1.0) } else { 0.0 };
        self.new_blocks.push((o.momentary, o.short_term));
    }
}
