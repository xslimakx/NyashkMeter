//! Six-section parametric EQ core shared by NyashkEQ and NyashkChannel:
//! high-pass (18 dB/oct), low shelf/bell, two bells, high shelf/bell, low-pass.
use crate::dsp::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Voicing {
    /// Constant-Q bells, tighter shelves.
    Black,
    /// Proportional-Q bells (narrower with more gain), broader shelves.
    Brown,
}

#[derive(Clone, Copy, Debug)]
pub struct Band {
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
    /// LF / HF only: bell instead of shelf.
    pub bell: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct EqCfg {
    pub hp_on: bool,
    pub hp_f: f32,
    pub lp_on: bool,
    pub lp_f: f32,
    pub lf: Band,
    pub lmf: Band,
    pub hmf: Band,
    pub hf: Band,
    pub voicing: Voicing,
}

impl Default for EqCfg {
    fn default() -> Self {
        let b = |f| Band { freq: f, gain_db: 0.0, q: 1.0, bell: false };
        Self { hp_on: false, hp_f: 30.0, lp_on: false, lp_f: 18000.0, lf: b(100.0), lmf: b(600.0), hmf: b(3000.0), hf: b(10000.0), voicing: Voicing::Black }
    }
}

#[derive(Clone, Copy)]
pub struct EqCoefs {
    pub hp1: Coef,
    pub hp2: Coef,
    pub hp_on: bool,
    pub lf: Coef,
    pub lmf: Coef,
    pub hmf: Coef,
    pub hf: Coef,
    pub lp: Coef,
    pub lp_on: bool,
}

fn eff_q(v: Voicing, q: f32, gain_db: f32) -> f32 {
    match v {
        Voicing::Black => q,
        // proportional Q: small gains are broad, big gains are narrow
        Voicing::Brown => q * (0.55 + 0.45 * (gain_db.abs() / 12.0).min(1.5)),
    }
}

impl EqCoefs {
    pub fn new(fs: f32, c: &EqCfg) -> Self {
        let shelf_q = match c.voicing {
            Voicing::Black => 0.9,
            Voicing::Brown => 0.6,
        };
        let band = |b: &Band, shelf_low: Option<bool>| -> Coef {
            if b.gain_db.abs() < 0.01 {
                return Coef::THRU;
            }
            match shelf_low {
                Some(low) if !b.bell => {
                    if low {
                        Coef::low_shelf(fs, b.freq, b.gain_db, shelf_q)
                    } else {
                        Coef::high_shelf(fs, b.freq, b.gain_db, shelf_q)
                    }
                }
                _ => Coef::peaking(fs, b.freq, b.gain_db, eff_q(c.voicing, b.q, b.gain_db)),
            }
        };
        Self {
            hp1: Coef::highpass1(fs, c.hp_f),
            hp2: Coef::highpass(fs, c.hp_f, 1.0),
            hp_on: c.hp_on,
            lf: band(&c.lf, Some(true)),
            lmf: band(&c.lmf, None),
            hmf: band(&c.hmf, None),
            hf: band(&c.hf, Some(false)),
            lp: Coef::lowpass(fs, c.lp_f, 0.7071),
            lp_on: c.lp_on,
        }
    }

    /// Total magnitude response in dB.
    pub fn response_db(&self, fs: f32, f: f32) -> f32 {
        let mut d = self.lf.response_db(fs, f) + self.lmf.response_db(fs, f) + self.hmf.response_db(fs, f) + self.hf.response_db(fs, f);
        if self.hp_on {
            d += self.hp1.response_db(fs, f) + self.hp2.response_db(fs, f);
        }
        if self.lp_on {
            d += self.lp.response_db(fs, f);
        }
        d
    }
}

#[derive(Clone, Default)]
pub struct EqState {
    hp1: Bq,
    hp2: Bq,
    lf: Bq,
    lmf: Bq,
    hmf: Bq,
    hf: Bq,
    lp: Bq,
}

impl EqState {
    #[inline]
    pub fn tick(&mut self, c: &EqCoefs, x: f32) -> f32 {
        let mut y = x;
        if c.hp_on {
            y = self.hp2.tick(&c.hp2, self.hp1.tick(&c.hp1, y));
        }
        y = self.lf.tick(&c.lf, y);
        y = self.lmf.tick(&c.lmf, y);
        y = self.hmf.tick(&c.hmf, y);
        y = self.hf.tick(&c.hf, y);
        if c.lp_on {
            y = self.lp.tick(&c.lp, y);
        }
        y
    }
    pub fn reset(&mut self) {
        *self = EqState::default();
    }
}
