//! NyashkMeter: loudness (LUFS / LRA), true peak, spectrum, stereo image and tonal balance (VST3).
//! The audio passes through untouched.

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use nyashk_common::tap::Tap;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

pub mod dsp;
mod editor;

pub const HISTORY: usize = 3000; // 5 minutes at 10 values per second

// indices into Shared::vals
pub const V_M: usize = 0;
pub const V_S: usize = 1;
pub const V_I: usize = 2;
pub const V_LRA: usize = 3;
pub const V_MAXM: usize = 4;
pub const V_MAXS: usize = 5;
pub const V_TP: usize = 6;
pub const V_TPL: usize = 7;
pub const V_TPR: usize = 8;
pub const V_PEAK: usize = 9;
pub const V_RMSL: usize = 10;
pub const V_RMSR: usize = 11;
pub const V_CORR: usize = 12;
pub const V_OVER: usize = 13;
const V_COUNT: usize = 14;

pub struct Shared {
    vals: Vec<AtomicU32>,
    pub tap_l: Tap,
    pub tap_r: Tap,
    pub tap_m: Tap,
    pub tap_s: Tap,
    hist_m: Vec<AtomicU32>,
    hist_s: Vec<AtomicU32>,
    pub hist_pos: AtomicUsize,
    pub reset: AtomicBool,
    pub fs: AtomicU32,
}

impl Shared {
    fn new() -> Self {
        let m = (-120f32).to_bits();
        Self {
            vals: (0..V_COUNT).map(|_| AtomicU32::new(m)).collect(),
            tap_l: Tap::new(16384),
            tap_r: Tap::new(16384),
            tap_m: Tap::new(16384),
            tap_s: Tap::new(16384),
            hist_m: (0..HISTORY).map(|_| AtomicU32::new(m)).collect(),
            hist_s: (0..HISTORY).map(|_| AtomicU32::new(m)).collect(),
            hist_pos: AtomicUsize::new(0),
            reset: AtomicBool::new(false),
            fs: AtomicU32::new(48000f32.to_bits()),
        }
    }
    pub fn get(&self, i: usize) -> f32 {
        f32::from_bits(self.vals[i].load(Ordering::Relaxed))
    }
    fn set(&self, i: usize, v: f32) {
        self.vals[i].store(v.to_bits(), Ordering::Relaxed);
    }
    fn push_hist(&self, m: f32, s: f32) {
        let p = self.hist_pos.load(Ordering::Relaxed);
        self.hist_m[p % HISTORY].store(m.to_bits(), Ordering::Relaxed);
        self.hist_s[p % HISTORY].store(s.to_bits(), Ordering::Relaxed);
        self.hist_pos.store(p.wrapping_add(1), Ordering::Release);
    }
    /// Last `n` history values (oldest first), momentary and short-term.
    pub fn history(&self, n: usize, m: &mut Vec<f32>, s: &mut Vec<f32>) {
        m.clear();
        s.clear();
        let n = n.min(HISTORY);
        let end = self.hist_pos.load(Ordering::Acquire);
        for i in 0..n {
            let idx = end.wrapping_sub(n - i) % HISTORY;
            m.push(f32::from_bits(self.hist_m[idx].load(Ordering::Relaxed)));
            s.push(f32::from_bits(self.hist_s[idx].load(Ordering::Relaxed)));
        }
    }
    pub fn published(&self) -> usize {
        self.hist_pos.load(Ordering::Acquire)
    }
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum SpecMode {
    #[name = "CURVE"]
    Curve,
    #[name = "1/3 OCT"]
    Third,
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum SpecSrc {
    #[name = "L+R"]
    Mix,
    #[name = "MID"]
    Mid,
    #[name = "SIDE"]
    Side,
    #[name = "L"]
    Left,
    #[name = "R"]
    Right,
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Slope {
    #[name = "0 dB"]
    Flat,
    #[name = "3 dB"]
    Three,
    #[name = "4.5 dB"]
    FourHalf,
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Speed {
    #[name = "FAST"]
    Fast,
    #[name = "MED"]
    Medium,
    #[name = "SLOW"]
    Slow,
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Target {
    #[name = "OFF"]
    Off,
    #[name = "-9 LOUD"]
    Loud,
    #[name = "-14 STREAM"]
    Stream,
    #[name = "-16 PODCAST"]
    Podcast,
    #[name = "-23 EBU"]
    Ebu,
}

impl Target {
    pub fn lufs(self) -> Option<f32> {
        match self {
            Target::Off => None,
            Target::Loud => Some(-9.0),
            Target::Stream => Some(-14.0),
            Target::Podcast => Some(-16.0),
            Target::Ebu => Some(-23.0),
        }
    }
}

#[derive(Enum, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Span {
    #[name = "1 MIN"]
    Min1,
    #[name = "5 MIN"]
    Min5,
}

#[derive(Params)]
pub struct MeterParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    #[id = "smode"]
    pub spec_mode: EnumParam<SpecMode>,
    #[id = "ssrc"]
    pub spec_src: EnumParam<SpecSrc>,
    #[id = "slope"]
    pub slope: EnumParam<Slope>,
    #[id = "speed"]
    pub speed: EnumParam<Speed>,
    #[id = "hold"]
    pub hold: BoolParam,
    #[id = "target"]
    pub target: EnumParam<Target>,
    #[id = "span"]
    pub span: EnumParam<Span>,
}

impl Default for MeterParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(1080, 570),
            spec_mode: EnumParam::new("Spectrum Mode", SpecMode::Curve),
            spec_src: EnumParam::new("Spectrum Source", SpecSrc::Mix),
            slope: EnumParam::new("Spectrum Slope", Slope::FourHalf),
            speed: EnumParam::new("Spectrum Speed", Speed::Medium),
            hold: BoolParam::new("Peak Hold", true),
            target: EnumParam::new("Loudness Target", Target::Stream),
            span: EnumParam::new("History Span", Span::Min1),
        }
    }
}

pub struct NyashkMeter {
    params: Arc<MeterParams>,
    shared: Arc<Shared>,
    meter: Option<dsp::Meter>,
}

impl Default for NyashkMeter {
    fn default() -> Self {
        Self { params: Arc::new(MeterParams::default()), shared: Arc::new(Shared::new()), meter: None }
    }
}

impl Plugin for NyashkMeter {
    const NAME: &'static str = "NyashkMeter";
    const VENDOR: &'static str = "Nyashk";
    const URL: &'static str = "https://github.com/xslimakx";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout { main_input_channels: NonZeroU32::new(2), main_output_channels: NonZeroU32::new(2), ..AudioIOLayout::const_default() },
        AudioIOLayout { main_input_channels: NonZeroU32::new(1), main_output_channels: NonZeroU32::new(1), ..AudioIOLayout::const_default() },
    ];
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _e: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.shared.clone())
    }

    fn initialize(&mut self, _l: &AudioIOLayout, config: &BufferConfig, _c: &mut impl InitContext<Self>) -> bool {
        self.meter = Some(dsp::Meter::new(config.sample_rate));
        self.shared.fs.store(config.sample_rate.to_bits(), Ordering::Relaxed);
        true
    }

    fn reset(&mut self) {
        if let Some(m) = self.meter.as_mut() {
            m.reset_filters();
        }
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _c: &mut impl ProcessContext<Self>) -> ProcessStatus {
        nyashk_common::demo::sanitize(buffer);
        #[cfg(feature = "standalone")]
        nyashk_common::demo::fill(buffer, 48000.0);
        let Some(meter) = self.meter.as_mut() else { return ProcessStatus::Normal };
        let sh = &self.shared;
        if sh.reset.swap(false, Ordering::Relaxed) {
            meter.reset_stats();
        }
        for frame in buffer.iter_samples() {
            let n = frame.len().min(2);
            let mut x = [0.0f32; 2];
            for (i, s) in frame.into_iter().enumerate().take(n) {
                x[i] = *s;
            }
            let (l, r) = (x[0], if n == 2 { x[1] } else { x[0] });
            sh.tap_l.push(l);
            sh.tap_r.push(r);
            sh.tap_m.push((l + r) * 0.5);
            sh.tap_s.push((l - r) * 0.5);
            meter.frame(&x, n);
        }
        let o = meter.out;
        sh.set(V_M, o.momentary);
        sh.set(V_S, o.short_term);
        sh.set(V_I, o.integrated);
        sh.set(V_LRA, o.lra);
        sh.set(V_MAXM, o.max_momentary);
        sh.set(V_MAXS, o.max_short);
        sh.set(V_TP, o.tp_max);
        sh.set(V_TPL, o.tp_l);
        sh.set(V_TPR, o.tp_r);
        sh.set(V_PEAK, o.peak_max);
        sh.set(V_RMSL, o.rms_l);
        sh.set(V_RMSR, o.rms_r);
        sh.set(V_CORR, o.corr);
        sh.set(V_OVER, o.tp_over as f32);
        for (m, s) in meter.new_blocks.drain(..) {
            sh.push_hist(m, s);
        }
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for NyashkMeter {
    const VST3_CLASS_ID: [u8; 16] = *b"NyashkMeterVST3!";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Analyzer];
}

nih_export_vst3!(NyashkMeter);
