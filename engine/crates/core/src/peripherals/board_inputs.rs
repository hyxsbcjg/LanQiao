//! Drawing-based V2 RB3 transfer; these values are not measured board calibration.
//!
//! V2 uses output feedback rather than the pin-7 astable topology. The model
//! uses the schematic timing values: R8 = 100 ohms, RB3 = 10 kohms, C14 = 100 nF.
//!
//! Assume a linear rheostat, ideal rail-to-rail output, and no wiper resistance
//! or charge-pump loading. Position increases resistance and lowers frequency;
//! physical clockwise direction is unverified. This is not the pin-7 astable.

/// Return RB3's resistance in ohms, excluding series R8.
/// Position 0 is zero resistance; position 1 is 10k. End stops clamp position.
pub(crate) fn rb3_resistance(pos: f32) -> f32 {
    pos.clamp(0.0, 1.0) * 10_000.0
}

/// Return ideal frequency in Hz: both half-cycles take ln(2) * (R8 + RB3) * C14.
pub(crate) fn rb3_frequency(pos: f32) -> f32 {
    1.0 / (2.0 * std::f32::consts::LN_2 * (100.0 + rb3_resistance(pos)) * 100e-9)
}

// V2 RD1.2=VCC, RD1.1=AIN1, R31=10k to GND (netlist source 18380-18381,
// 18632-18633). Device META 5064: GL5516, 5-10k at 10 lux, dark 500k.
// Assumptions: midpoint 7.5k at 10 lux, gamma .7, 0..1 maps to 0..500 lux.
// ponytail: static nominal LDR curve; calibrate R10/gamma/lux ceiling against a real board if needed.
pub(crate) fn ldr_resistance(level: f32) -> f32 {
    if level <= 0.0 {
        500_000.0
    } else {
        (7500.0 * (10.0 / (500.0 * level)).powf(0.7)).min(500_000.0)
    }
}

pub(crate) fn light_voltage(level: f32) -> f32 {
    5.0 * 10_000.0 / (10_000.0 + ldr_resistance(level))
}

// V2 RB2.1=GND, .3=VCC, .2=AIN3 (source 18387-18389). Unloaded linear wiper.
pub(crate) fn rb2_voltage(position: f32) -> f32 {
    5.0 * position
}

#[cfg(test)]
mod tests {
    use super::{ldr_resistance, light_voltage, rb2_voltage, rb3_frequency, rb3_resistance};

    #[test]
    fn light_divider_rises_with_illuminance() {
        for (level, resistance, voltage) in [
            (0.0, 500_000.0, 0.09803922),
            (0.02, 7500.0, 2.857143),
            (1.0, 485.045, 4.7686967),
        ] {
            assert!((ldr_resistance(level) - resistance).abs() < 0.05);
            assert!((light_voltage(level) - voltage).abs() < 0.0001);
        }
    }

    #[test]
    fn rb2_divider_spans_ground_to_vcc() {
        for (position, voltage) in [
            (0.0, 0.0),
            (0.25, 1.25),
            (0.5, 2.5),
            (0.75, 3.75),
            (1.0, 5.0),
        ] {
            assert_eq!(rb2_voltage(position), voltage);
        }
    }

    #[test]
    fn rb3_resistance_follows_linear_dial_and_end_stops() {
        for (pos, expected) in [
            (f32::NEG_INFINITY, 0.0),
            (-0.1, 0.0),
            (0.0, 0.0),
            (0.25, 2_500.0),
            (0.5, 5_000.0),
            (0.75, 7_500.0),
            (1.0, 10_000.0),
            (1.1, 10_000.0),
            (f32::INFINITY, 10_000.0),
        ] {
            assert_eq!(rb3_resistance(pos), expected, "position {pos}");
        }
    }

    #[test]
    fn rb3_frequency_matches_output_feedback_rc() {
        // Independent numerical expectations use 10k, 100 ohms, and 100 nF.
        for (pos, expected_hz) in [
            (-0.1, 72_134.75),
            (0.0, 72_134.75),
            (0.25, 2_774.4136),
            (0.5, 1_414.4069),
            (0.75, 949.1415),
            (1.0, 714.20544),
            (1.1, 714.20544),
        ] {
            let actual_hz = rb3_frequency(pos);
            assert!(
                (actual_hz - expected_hz).abs() < 0.02,
                "position {pos}: expected {expected_hz} Hz, got {actual_hz} Hz"
            );
        }
    }
}
