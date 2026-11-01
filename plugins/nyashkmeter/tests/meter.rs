use nyashkmeter::dsp::*;

fn feed(m: &mut Meter, secs: f32, f: f32, db_l: f32, db_r: f32) {
    let fs = m.fs();
    let (al, ar) = (10f32.powf(db_l / 20.0), 10f32.powf(db_r / 20.0));
    for n in 0..(secs * fs) as usize {
        let s = (2.0 * std::f32::consts::PI * f * n as f32 / fs).sin();
        m.frame(&[s * al, s * ar], 2);
    }
}

#[test]
fn k_weighting_matches_the_standard_at_48k() {
    let c = k_filters(48000.0);
    assert!((c[0].b0 - 1.53512485958697).abs() < 1e-4);
    assert!((c[0].b1 + 2.69169618940638).abs() < 1e-4);
    assert!((c[0].b2 - 1.19839281085285).abs() < 1e-4);
    assert!((c[0].a1 + 1.69065929318241).abs() < 1e-4);
    assert!((c[0].a2 - 0.73248077421585).abs() < 1e-4);
    assert!((c[1].a1 + 1.99004745483398).abs() < 1e-4);
    assert!((c[1].a2 - 0.99007225036621).abs() < 1e-4);
}

#[test]
fn reference_tone_reads_minus_23() {
    // EBU Tech 3341: 1 kHz stereo sine at -23 dBFS reads -23.0 LUFS (all three time scales)
    for fs in [44100.0, 48000.0, 96000.0] {
        let mut m = Meter::new(fs);
        feed(&mut m, 12.0, 1000.0, -23.0, -23.0);
        assert!((m.out.momentary + 23.0).abs() < 0.15, "fs {fs}: M {}", m.out.momentary);
        assert!((m.out.short_term + 23.0).abs() < 0.15, "S {}", m.out.short_term);
        assert!((m.out.integrated + 23.0).abs() < 0.15, "I {}", m.out.integrated);
    }
}

#[test]
fn silence_and_quiet_parts_are_gated() {
    let mut m = Meter::new(48000.0);
    feed(&mut m, 6.0, 1000.0, -23.0, -23.0);
    feed(&mut m, 6.0, 1000.0, -90.0, -90.0); // below the absolute gate
    assert!((m.out.integrated + 23.0).abs() < 0.2, "{}", m.out.integrated);
    feed(&mut m, 8.0, 1000.0, -40.0, -40.0); // 17 LU quieter: below the relative gate
    assert!((m.out.integrated + 23.0).abs() < 0.3, "{}", m.out.integrated);
}

#[test]
fn loudness_range_of_two_levels() {
    let mut m = Meter::new(48000.0);
    feed(&mut m, 20.0, 1000.0, -30.0, -30.0);
    feed(&mut m, 20.0, 1000.0, -20.0, -20.0);
    assert!((m.out.lra - 10.0).abs() < 1.2, "LRA {}", m.out.lra);
    let mut steady = Meter::new(48000.0);
    feed(&mut steady, 20.0, 1000.0, -20.0, -20.0);
    assert!(steady.out.lra < 0.5, "{}", steady.out.lra);
}

#[test]
fn reset_clears_statistics() {
    let mut m = Meter::new(48000.0);
    feed(&mut m, 5.0, 1000.0, -10.0, -10.0);
    assert!(m.out.max_momentary > -12.0);
    m.reset_stats();
    assert!(m.out.integrated < -100.0 && m.out.max_momentary < -100.0 && m.out.lra == 0.0);
}

#[test]
fn true_peak_sees_intersample_overs() {
    // a sine at fs/4 sampled 45 degrees off its crest: every sample is 0.707 but the real peak is 1.0
    let mut m = Meter::new(48000.0);
    for n in 0..48000 {
        let s = (std::f32::consts::FRAC_PI_2 * n as f32 + std::f32::consts::FRAC_PI_4).sin();
        m.frame(&[s, s], 2);
    }
    assert!(m.out.peak_max < -2.9 && m.out.peak_max > -3.2, "sample peak {}", m.out.peak_max);
    assert!(m.out.tp_max > -0.3 && m.out.tp_max < 0.3, "true peak {}", m.out.tp_max);
    assert!(m.out.tp_over >= 1);
}

#[test]
fn correlation_and_balance() {
    let mut m = Meter::new(48000.0);
    feed(&mut m, 2.0, 700.0, -12.0, -12.0);
    assert!(m.out.corr > 0.99, "{}", m.out.corr);
    let mut inv = Meter::new(48000.0);
    for n in 0..96000 {
        let s = 0.3 * (2.0 * std::f32::consts::PI * 700.0 * n as f32 / 48000.0).sin();
        inv.frame(&[s, -s], 2);
    }
    assert!(inv.out.corr < -0.99, "{}", inv.out.corr);
    // uncorrelated noise
    let mut nz = Meter::new(48000.0);
    let (mut a, mut b) = (12345u32, 987654u32);
    let mut rnd = |s: &mut u32| {
        *s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        (*s >> 8) as f32 / 8388608.0 - 1.0
    };
    for _ in 0..192000 {
        nz.frame(&[0.3 * rnd(&mut a), 0.3 * rnd(&mut b)], 2);
    }
    assert!(nz.out.corr.abs() < 0.15, "{}", nz.out.corr);
    let mut lopsided = Meter::new(48000.0);
    feed(&mut lopsided, 2.0, 700.0, -10.0, -16.0);
    assert!((lopsided.out.rms_l - lopsided.out.rms_r - 6.0).abs() < 0.3);
}

#[test]
fn mono_input_is_measured() {
    let mut m = Meter::new(48000.0);
    for n in 0..(12.0 * 48000.0) as usize {
        let s = 0.07 * (2.0 * std::f32::consts::PI * 1000.0 * n as f32 / 48000.0).sin();
        m.frame(&[s, 0.0], 1);
    }
    // one channel at -23 dBFS peak is 3 LU below the stereo reference
    assert!((m.out.integrated + 26.0 - (20.0 * 0.07f32.log10() + 23.0)).abs() < 0.5, "{}", m.out.integrated);
}
