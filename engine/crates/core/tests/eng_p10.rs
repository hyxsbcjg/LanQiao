use stcjudge::{KeyId, KeyMode, Simulator, WaveCaptureOptions};

// STC15_DS.pdf p.324, section 4.9.1: a quasi-bidirectional latch low
// strongly sinks the pin. SCH_V31.pdf pp.1/4: P1.0 is US_TX, not US_RX.
#[test]
fn eng_p10_pin_read_inversion_releases_a_low_latch() -> anyhow::Result<()> {
    let mut sim = Simulator::from_code_with_options(
        vec![
            0xa2, 0x90, // MOV C, P1.0: ordinary pin read
            0xb3, // CPL C
            0x92, 0x90, // MOV P1.0, C
        ],
        false,
        WaveCaptureOptions::default(),
    );
    sim.poke_sfr(0x90, 0xfe)?;
    for _ in 0..3 {
        sim.step_instruction()?;
    }
    assert_eq!(sim.peek_sfr_latch(0x90)?, 0xff);
    Ok(())
}

#[test]
fn eng_p10_pin_read_inversion_keeps_toggling_without_clobbering_other_bits() -> anyhow::Result<()> {
    for initial_latch in [0xa4, 0xa5] {
        let mut sim = Simulator::from_code_with_options(
            vec![
                0xa2, 0x90, // MOV C, P1.0
                0xb3, // CPL C
                0x92, 0x90, // MOV P1.0, C
                0x80, 0xf9, // SJMP back to MOV C, P1.0
            ],
            false,
            WaveCaptureOptions::default(),
        );
        sim.set_distance_cm(17.0);
        sim.poke_sfr(0x90, initial_latch)?;
        let mut expected_latch = initial_latch;
        for _ in 0..8 {
            sim.step_instruction()?;
            assert_eq!(sim.peek_sfr(0xd0)? & 0x80 != 0, expected_latch & 1 != 0);
            sim.step_instruction()?;
            assert_eq!(sim.peek_sfr(0xd0)? & 0x80 != 0, expected_latch & 1 == 0);
            sim.step_instruction()?;
            expected_latch ^= 1;
            assert_eq!(sim.peek_sfr_latch(0x90)?, expected_latch);
            assert_eq!(sim.peek_sfr(0x90)? & 1, expected_latch & 1);
            sim.step_instruction()?;
        }
    }
    Ok(())
}

#[test]
fn eng_p10_latch_levels_preserve_other_pins_on_both_key_paths() -> anyhow::Result<()> {
    for mode in [KeyMode::Keyboard, KeyMode::Button] {
        for pressed in [false, true] {
            let mut sim = Simulator::nop(false);
            sim.key_mode(mode);
            sim.poke_sfr(0xc0, 0xef)?; // Drive keyboard COL1 low.
            sim.set_key_id(KeyId::S4, pressed);
            let before = sim.snapshot();
            for latch in 0..=u8::MAX {
                sim.poke_sfr(0x90, latch)?;
                // No time advances: DS18B20 is idle and ultrasonic RX stays high.
                assert_eq!(sim.peek_sfr(0x90)?, latch | 0x02);
                assert_eq!(sim.peek_sfr_latch(0x90)?, latch);
                let after = sim.snapshot();
                assert_eq!(after.port_input[1], latch | 0x02);
                for port in [0, 2, 3, 4, 5] {
                    assert_eq!(after.port_input[port], before.port_input[port]);
                    assert_eq!(after.port_latch[port], before.port_latch[port]);
                }
            }
        }
    }
    Ok(())
}

#[test]
fn eng_p10_pin_read_inversion_triggers_ultrasonic_echo() -> anyhow::Result<()> {
    let mut sim = Simulator::from_code_with_options(
        vec![
            0xa2, 0x90, 0xb3, 0x92, 0x90, // Invert low to high through a pin read.
            0xa2, 0x90, 0xb3, 0x92, 0x90, // Invert high to low through a pin read.
            0x80, 0xfe, // Hold TX low while the existing device model completes echo.
        ],
        false,
        WaveCaptureOptions::default(),
    );
    sim.set_distance_cm(17.0);
    sim.poke_sfr(0x90, 0xfe)?;
    for _ in 0..3 {
        sim.step_instruction()?;
    }
    assert_eq!(sim.peek_sfr(0x90)? & 0x03, 0x03);
    for _ in 0..3 {
        sim.step_instruction()?;
    }
    assert_eq!(sim.peek_sfr(0x90)? & 0x03, 0x02);
    // Existing UltrasonicDevice contract: 17 cm completes in about 1 ms.
    // This checks electrical trigger propagation, not analog transducer timing.
    sim.run_us(1_100)?;
    assert_eq!(sim.peek_sfr(0x90)? & 0x03, 0x00);
    assert_eq!(sim.peek_sfr_latch(0x90)?, 0xfe);
    Ok(())
}
