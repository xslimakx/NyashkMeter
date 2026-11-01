//! Small radix-2 FFT (no dependencies) for the analyzer.

pub struct Fft {
    n: usize,
    cos: Vec<f32>,
    sin: Vec<f32>,
    rev: Vec<usize>,
    pub window: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
}

impl Fft {
    /// `n` must be a power of two.
    pub fn new(n: usize) -> Self {
        assert!(n.is_power_of_two());
        let bits = n.trailing_zeros();
        let rev = (0..n).map(|i| i.reverse_bits() >> (usize::BITS - bits)).collect();
        let cos = (0..n / 2).map(|k| (-2.0 * std::f64::consts::PI * k as f64 / n as f64).cos() as f32).collect();
        let sin = (0..n / 2).map(|k| (-2.0 * std::f64::consts::PI * k as f64 / n as f64).sin() as f32).collect();
        // Hann window
        let window = (0..n).map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos() as f32).collect();
        Self { n, cos, sin, rev, window, re: vec![0.0; n], im: vec![0.0; n] }
    }

    pub fn len(&self) -> usize {
        self.n
    }

    /// Windowed magnitude spectrum of `frame` (length n) into `out` (length n/2), scaled so a full-scale
    /// sine reads 0 dB in its bin (coherent gain of the Hann window compensated).
    pub fn magnitude_db(&mut self, frame: &[f32], out: &mut [f32]) {
        let n = self.n;
        for i in 0..n {
            self.re[self.rev[i]] = frame[i] * self.window[i];
            self.im[self.rev[i]] = 0.0;
        }
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let step = n / len;
            let mut start = 0;
            while start < n {
                for k in 0..half {
                    let (c, s) = (self.cos[k * step], self.sin[k * step]);
                    let (a, b) = (start + k, start + k + half);
                    let tr = self.re[b] * c - self.im[b] * s;
                    let ti = self.re[b] * s + self.im[b] * c;
                    self.re[b] = self.re[a] - tr;
                    self.im[b] = self.im[a] - ti;
                    self.re[a] += tr;
                    self.im[a] += ti;
                }
                start += len;
            }
            len *= 2;
        }
        // Hann coherent gain is 0.5; a sine of amplitude A gives |X| = A * n/4 -> normalise to A
        let norm = 4.0 / n as f32;
        for k in 0..n / 2 {
            let m = (self.re[k] * self.re[k] + self.im[k] * self.im[k]).sqrt() * norm;
            out[k] = 20.0 * (m + 1e-9).log10();
        }
    }
}
