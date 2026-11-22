use std::path::Path;

use stcjudge::{Brightness, ResetMode, Simulator, WaveCaptureOptions};

fn machine() -> Simulator {
    let mut sim =
        Simulator::from_code_with_options(vec![0x80, 0xFE], false, WaveCaptureOptions::default());
    // J13 MM closes the GPIO latches while /WR is high, retaining external writes.
    sim.set_latch_mode("MM").unwrap();
    sim
}

fn dark(sim: &mut Simulator) {
    sim.poke_xdata(0x8000, 0xFF);
    sim.poke_xdata(0xC000, 0);
    sim.poke_xdata(0xE000, 0xFF);
    sim.step_instruction().unwrap();
    sim.take_brightness();
}

fn near(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual={actual}, expected={expected}, tolerance={tolerance}"
    );
}

// SCH_V31.pdf p3: U6 is active-low LED; U8 COM is active-high;
// U7 A..DP is active-low. 74HC_HCT573.pdf p1, p6 Table 4: Q holds its value.
// At 12 MHz the existing SJMP model uses 2 clocks (166.67ns). Allow two
// instruction boundaries per slot, plus f32 rounding, rather than a magic %.
fn tolerance(window_ns: u64, slots: u64) -> f32 {
    assert!(window_ns > 0);
    (2.0 * slots as f64 * 2.0 * 1e9 / 12e6 / window_ns as f64) as f32 + 1e-6
}

#[test]
fn brightness_eight_digit_scan_in_both_latch_orders() {
    for segment_first in [false, true] {
        let mut sim = machine();
        dark(&mut sim);
        for digit in 0..8 {
            let writes = [(0xC000, 1 << digit), (0xE000, !(1 << digit))];
            for index in if segment_first { [1, 0] } else { [0, 1] } {
                sim.poke_xdata(writes[index].0, writes[index].1);
                sim.step_instruction().unwrap();
            }
            sim.run_us(1_000).unwrap();
        }
        let value = sim.take_brightness();
        for digit in 0..8 {
            for segment in 0..8 {
                near(
                    value.seg[digit][segment],
                    if digit == segment { 0.125 } else { 0.0 },
                    tolerance(value.window_ns, 8),
                );
            }
        }
        assert_eq!(value.led, [0.0; 8]);
    }
}

#[test]
fn brightness_counts_ten_us_ghost_without_dimming_shared_segment() {
    for digit_count in [1, 8] {
        let mut sim = machine();
        dark(&mut sim);
        for digit in 0..digit_count {
            sim.poke_xdata(0xC000, 1 << digit);
            sim.poke_xdata(0xE000, !0x42);
            sim.run_us(10).unwrap();
            sim.poke_xdata(0xE000, !0x02);
            sim.run_us(990).unwrap();
        }
        let value = sim.take_brightness();
        for digit in 0..digit_count {
            near(
                value.seg[digit][6],
                0.01 / digit_count as f32,
                tolerance(value.window_ns, 1),
            );
            near(
                value.seg[digit][1],
                1.0 / digit_count as f32,
                tolerance(value.window_ns, 1),
            );
            near(value.seg[digit][0], 0.0, 1e-6);
        }
    }
}

#[test]
fn brightness_slow_scan_has_different_successive_33ms_windows() {
    let mut sim = machine();
    dark(&mut sim);
    sim.poke_xdata(0xE000, !0x80);
    for digit in 0..3 {
        sim.poke_xdata(0xC000, 1 << digit);
        sim.run_us(10_000).unwrap();
    }
    sim.poke_xdata(0xC000, 8);
    sim.run_us(3_000).unwrap();
    let first = sim.take_brightness();
    sim.run_us(7_000).unwrap();
    for digit in 4..6 {
        sim.poke_xdata(0xC000, 1 << digit);
        sim.run_us(10_000).unwrap();
    }
    sim.poke_xdata(0xC000, 64);
    sim.run_us(6_000).unwrap();
    let second = sim.take_brightness();
    assert_eq!(first.window_ns, 33_000_000);
    assert_eq!(second.window_ns, 33_000_000);
    near(first.seg[0][7], 10.0 / 33.0, tolerance(first.window_ns, 1));
    assert_eq!(second.seg[0][7], 0.0);
    near(second.seg[6][7], 6.0 / 33.0, tolerance(second.window_ns, 1));
}

#[test]
fn brightness_led_ten_percent_pwm_and_sample_hex() {
    let mut sim = machine();
    dark(&mut sim);
    for _ in 0..100 {
        sim.poke_xdata(0x8000, 0xFE);
        sim.run_us(100).unwrap();
        sim.poke_xdata(0x8000, 0xFF);
        sim.run_us(900).unwrap();
    }
    let value = sim.take_brightness();
    near(value.led[0], 0.1, tolerance(value.window_ns, 100));
    assert_eq!(&value.led[1..], &[0.0; 7]);

    // README: 1kHz PWM. main.c: 100us T0, led_cur=0..9,
    // led_cur <= led_duty, initial led_duty=0: one of ten slots is on.
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/led_pwm/prj/Objects/led_pwm.hex");
    let mut sample = Simulator::from_hex_path(&path, false).unwrap();
    sample.run_ms(100).unwrap();
    sample.take_brightness();
    sample.run_ms(100).unwrap();
    let value = sample.take_brightness();
    assert!(value.window_ns > 0);
    // Include one PWM period at each window edge. ISR instruction latency is
    // below this two-period bound over a 100ms observation window.
    near(
        value.led[0],
        0.1,
        2.0 * 1_000_000.0 / value.window_ns as f32,
    );
    assert_eq!(&value.led[1..], &[0.0; 7]);
}

fn assert_empty(value: Brightness) {
    assert_eq!(value.window_ns, 0);
    assert_eq!(value.seg, [[0.0; 8]; 8]);
    assert_eq!(value.led, [0.0; 8]);
}

#[test]
fn brightness_reads_only_drain_integrals_and_resets_clear_at_retained_time() {
    let mut sim = machine();
    dark(&mut sim);
    sim.poke_xdata(0xC000, 3);
    sim.poke_xdata(0xE000, 0);
    sim.poke_xdata(0x8000, 0);
    sim.run_ms(1).unwrap();
    sim.take_brightness();
    sim.run_ms(101).unwrap();
    let before = sim.snapshot_text();
    let persistent = sim.export_persistent_state();
    let value = sim.take_brightness();
    assert_eq!(sim.snapshot_text(), before);
    assert_eq!(sim.export_persistent_state(), persistent);
    assert_eq!(value.seg[0], [1.0; 8]);
    assert_eq!(value.seg[1], [1.0; 8]);
    assert_eq!(value.seg[2], [0.0; 8]);
    assert_eq!(value.led, [1.0; 8]);
    assert!(sim.seg_raw(1).is_err()); // Judge expiry does not turn physical Q off.
    assert_empty(sim.take_brightness());
    for mode in [ResetMode::Cpu, ResetMode::Power] {
        sim.run_ms(1).unwrap();
        let time = sim.sim_time_ns();
        sim.reset_with_mode(mode).unwrap();
        assert_eq!(sim.sim_time_ns(), time);
        assert_empty(sim.take_brightness());
        sim.run_ms(1).unwrap();
        let value = sim.take_brightness();
        assert_eq!(value.window_ns, 1_000_000);
        assert_eq!(value.led, [1.0; 8]);
        assert_eq!(
            value.seg[0],
            if mode == ResetMode::Cpu {
                [1.0; 8]
            } else {
                [0.0; 8]
            }
        );
    }
}

#[test]
fn brightness_follows_real_gpio_and_movx_latch_writes() {
    let programs = [
        vec![
            0x75, 0x80, 0xFE, 0x75, 0xA0, 0x80, 0x75, 0xA0, 0, 0x75, 0x80, 1, 0x75, 0xA0, 0xC0,
            0x75, 0xA0, 0, 0x75, 0x80, 0x7F, 0x75, 0xA0, 0xE0, 0x75, 0xA0, 0, 0x80, 0xFE,
        ],
        vec![
            0x90, 0x80, 0, 0x74, 0xFE, 0xF0, 0x90, 0xC0, 0, 0x74, 1, 0xF0, 0x90, 0xE0, 0, 0x74,
            0x7F, 0xF0, 0x80, 0xFE,
        ],
    ];
    for (program, mode) in programs.into_iter().zip(["IO", "MM"]) {
        let mut sim =
            Simulator::from_code_with_options(program, false, WaveCaptureOptions::default());
        sim.set_latch_mode(mode).unwrap();
        for _ in 0..9 {
            sim.step_instruction().unwrap();
        }
        sim.take_brightness();
        sim.run_ms(1).unwrap();
        let value = sim.take_brightness();
        assert_eq!(value.led, [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(value.seg[0], [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        assert_eq!(value.seg[1..], [[0.0; 8]; 7]);
    }
}

// V2 J13/U24/U25 and 74HC_HCT573.pdf p6 Table 4: IO opens selected Q;
// resetting P0/P2 to FF opens SEG with all segments off, without clearing LED Q.
#[test]
fn brightness_follows_transparent_q_across_j13_switch_and_cpu_reset() {
    let mut sim = machine();
    sim.poke_xdata(0x8000, 0xFE);
    sim.poke_xdata(0xC000, 1);
    sim.poke_xdata(0xE000, 0xFE);
    sim.poke_sfr(0xA0, 0xE0).unwrap();
    sim.poke_sfr(0x80, 0xFD).unwrap();
    sim.take_brightness();
    sim.run_us(100).unwrap();
    assert_eq!(sim.snapshot().board_latches_effective[3], 0xFE);

    let time = sim.sim_time_ns();
    sim.set_latch_mode("IO").unwrap();
    assert_eq!(sim.sim_time_ns(), time);
    assert_eq!(sim.snapshot().board_latches_effective[3], 0xFD);
    sim.run_us(300).unwrap();
    let before = sim.snapshot_text();
    let value = sim.take_brightness();
    assert_eq!(sim.snapshot_text(), before);
    near(value.seg[0][0], 0.25, tolerance(value.window_ns, 2));
    near(value.seg[0][1], 0.75, tolerance(value.window_ns, 2));
    assert_eq!(value.seg[0][2..], [0.0; 6]);
    assert_eq!(value.seg[1..], [[0.0; 8]; 7]);
    assert_eq!(value.led, [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);

    sim.reset_with_mode(ResetMode::Cpu).unwrap();
    assert_eq!(sim.snapshot().board_latches_effective[3], 0xFF);
    assert_empty(sim.take_brightness());
    sim.run_ms(1).unwrap();
    let value = sim.take_brightness();
    assert_eq!(value.seg, [[0.0; 8]; 8]);
    assert_eq!(value.led, [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
}
