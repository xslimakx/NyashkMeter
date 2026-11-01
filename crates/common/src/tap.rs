//! Lock-free ring buffer the audio thread writes into and the GUI reads for analysis displays.
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

pub struct Tap {
    buf: Vec<AtomicU32>,
    pos: AtomicUsize,
}

impl Tap {
    pub fn new(len: usize) -> Self {
        Self { buf: (0..len).map(|_| AtomicU32::new(0)).collect(), pos: AtomicUsize::new(0) }
    }
    #[inline]
    pub fn push(&self, x: f32) {
        let p = self.pos.load(Ordering::Relaxed);
        self.buf[p % self.buf.len()].store(x.to_bits(), Ordering::Relaxed);
        self.pos.store(p.wrapping_add(1), Ordering::Release);
    }
    /// Copy the most recent `out.len()` samples, oldest first.
    pub fn snapshot(&self, out: &mut [f32]) {
        let n = out.len().min(self.buf.len());
        let end = self.pos.load(Ordering::Acquire);
        for (i, o) in out.iter_mut().enumerate().take(n) {
            let idx = end.wrapping_sub(n - i);
            *o = f32::from_bits(self.buf[idx % self.buf.len()].load(Ordering::Relaxed));
        }
    }
}
