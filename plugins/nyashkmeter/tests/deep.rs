//! The meter against the EBU Tech 3341 / 3342 style test signals and true-peak cases.
use nyashkmeter::dsp::Meter;
use std::f32::consts::PI;

fn run_segments(fs: f32, segs: &[(f32, f32)], f: f32) -> Meter {
    // (seconds, dBFS per channel) stereo 1 kHz sines
    let mut m = Meter::new(fs);
    let mut i = 0usize;
    for &(secs, dbfs) in segs {
        let a = 10f32.powf(dbfs / 20.0);
        for _ in 0..(secs * fs) as usize {
            let s = a * (2.0 * PI * f * i as f32 / fs).sin();
            m.frame(&[s, s], 2);
            i += 1;
        }
    }
    m
}

#[test]
fn tech3341_cases() {
    for fs in [44100.0f32, 48000.0, 96000.0] {
        // case 1 / 2: steady tones
        for lvl in [-23.0f32, -33.0] {
            let m = run_segments(fs, &[(20.0, lvl)], 1000.0);
            let o = m.out;
            assert!((o.momentary - lvl).abs() < 0.1 && (o.short_term - lvl).abs() < 0.1 && (o.integrated - lvl).abs() < 0.1, "fs {fs} {lvl}: {o:?}");
        }
        // case 3: -36 / -23 / -36 -> I -23 (relative gate removes the quiet parts)
        let m = run_segments(fs, &[(10.0, -36.0), (60.0, -23.0), (10.0, -36.0)], 1000.0);
        assert!((m.out.integrated + 23.0).abs() < 0.1, "fs {fs} case 3: {}", m.out.integrated);
        // case 4: -72 / -36 / -23 / -36 / -72 -> I -23 (absolute gate)
        let m = run_segments(fs, &[(10.0, -72.0), (10.0, -36.0), (60.0, -23.0), (10.0, -36.0), (10.0, -72.0)], 1000.0);
        assert!((m.out.integrated + 23.0).abs() < 0.1, "fs {fs} case 4: {}", m.out.integrated);
        // case 5: -26 / -20 / -26 -> I -23
        let m = run_segments(fs, &[(20.0, -26.0), (20.1, -20.0), (20.0, -26.0)], 1000.0);
        println!("fs {fs}: case 5 I {:.2}", m.out.integrated);
        assert!((m.out.integrated + 23.0).abs() < 0.15, "fs {fs} case 5: {}", m.out.integrated);
    }
}

#[test]
fn tech3342_loudness_range() {
    let fs = 48000.0;
    for (segs, want) in [
        (vec![(20.0f32, -20.0f32), (20.0, -30.0)], 10.0f32),
        (vec![(20.0, -20.0), (20.0, -15.0)], 5.0),
        (vec![(20.0, -40.0), (20.0, -20.0)], 20.0),
        (vec![(20.0, -50.0), (20.0, -35.0), (20.0, -20.0), (20.0, -35.0), (20.0, -50.0)], 15.0),
    ] {
        let m = run_segments(fs, &segs, 1000.0);
        println!("LRA {:.2} (expected {want})", m.out.lra);
        assert!((m.out.lra - want).abs() < 1.0, "LRA {} vs {want}", m.out.lra);
    }
}

#[test]
fn true_peak_cases() {
    // a sine whose samples miss its crests: true peak must read the crest, not the samples
    let fs = 48000.0;
    for (f, phase) in [(12000.0f32, PI / 4.0), (11999.0, 0.3), (8000.0, 0.5), (19000.0, 1.0), (997.0, 0.0)] {
        let mut m = Meter::new(fs);
        let a = 10f32.powf(-6.0 / 20.0);
        let mut smax = 0.0f32;
        for i in 0..(fs * 2.0) as usize {
            let s = a * (2.0 * PI * f * i as f32 / fs + phase).sin();
            smax = smax.max(s.abs());
            m.frame(&[s, s], 2);
        }
        let tp = m.out.tp_max;
        println!("{f} Hz: sample peak {:.2} dBFS, true peak {tp:.2} dBTP (actual -6.00)", 20.0 * smax.log10());
        assert!(tp > -6.0 - 0.4 && tp < -6.0 + 0.2, "{f} Hz: {tp}");
    }
}
