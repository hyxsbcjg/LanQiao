use stcjudge::Simulator;

// V2 board connections: J13, U24/U25, U6-U9; 74HC_HCT573.pdf p.6 Table 4.
#[test]
fn j13_io_latch_follows_p0_until_deselected() {
    let mut sim = Simulator::nop(false);
    sim.poke_sfr(0xA0, 0).unwrap();
    sim.poke_sfr(0x80, 0xFF).unwrap();
    sim.poke_sfr(0xA0, 0x80).unwrap();
    sim.poke_sfr(0x80, 0xFE).unwrap();
    assert_eq!(sim.snapshot().board_latches_effective[0], 0xFE);
    sim.poke_sfr(0xA0, 0).unwrap();
    sim.poke_sfr(0x80, 0).unwrap();
    assert_eq!(sim.snapshot().board_latches_effective[0], 0xFE);
}

#[test]
fn j13_nor_gate_truth_table_and_mode_changes_preserve_all_q() {
    for mode in ["IO", "MM"] {
        for wr_high in [false, true] {
            let mut sim = Simulator::nop(false);
            sim.poke_sfr(0xA0, 0).unwrap();
            sim.set_latch_mode(mode).unwrap();
            sim.poke_sfr(0xC0, if wr_high { 0xFF } else { 0xFB })
                .unwrap();
            let before = sim.snapshot().board_latches_effective;
            for select in 0..8u8 {
                sim.poke_sfr(0x80, 0x40 + select).unwrap();
                sim.poke_sfr(0xA0, select << 5).unwrap();
                sim.poke_sfr(0x80, 0x10 + select).unwrap();
                let expected = if select >= 4 && (mode == "IO" || !wr_high) {
                    0x10 + select
                } else if select >= 4 {
                    before[usize::from(select - 4)]
                } else {
                    continue;
                };
                assert_eq!(
                    sim.snapshot().board_latches_effective[usize::from(select - 4)],
                    expected
                );
            }
            sim.poke_sfr(0xA0, 0).unwrap();
            let held = sim.snapshot().board_latches_effective;
            sim.set_latch_mode(if mode == "IO" { "MM" } else { "IO" })
                .unwrap();
            assert_eq!(sim.snapshot().board_latches_effective, held);
            assert!(sim.set_latch_mode("invalid").is_err());
        }
    }
}

#[test]
fn j13_mm_external_movx_aliases_and_internal_xram() {
    for address in [
        0x8000u16, 0x9FFF, 0xA000, 0xBFFF, 0xC000, 0xDFFF, 0xE000, 0xFFFF,
    ] {
        // MOV DPTR,#address; MOV A,#5A; MOVX @DPTR,A; SJMP $.
        let code = vec![
            0x90,
            (address >> 8) as u8,
            address as u8,
            0x74,
            0x5A,
            0xF0,
            0x80,
            0xFE,
        ];
        let mut sim = Simulator::from_code_with_options(code, false, Default::default());
        sim.set_latch_mode("MM").unwrap();
        sim.run_us(10).unwrap();
        assert_eq!(
            sim.snapshot().board_latches_effective[usize::from((address >> 13) - 4)],
            0x5A
        );
        sim.poke_sfr(0xA0, 0x80).unwrap();
        sim.poke_sfr(0x80, 0).unwrap();
        assert_eq!(
            sim.snapshot().board_latches_effective[usize::from((address >> 13) - 4)],
            0x5A
        );
    }
    // Short MOVX stays internal when AUXR.EXTRAM=0, even with P2 selecting LED.
    let mut sim = Simulator::from_code_with_options(
        vec![0x78, 0x12, 0x74, 0x5A, 0xF2, 0x80, 0xFE],
        false,
        Default::default(),
    );
    sim.set_latch_mode("MM").unwrap();
    sim.poke_sfr(0xA0, 0x80).unwrap();
    sim.run_us(10).unwrap();
    assert_eq!(sim.snapshot().board_latches_effective[0], 0);
    assert_eq!(sim.peek_xdata(0x12), 0x5A);
}

#[test]
fn j13_mode_and_latches_survive_cpu_reset() {
    let mut sim = Simulator::nop(false);
    sim.set_latch_mode("MM").unwrap();
    sim.poke_xdata(0x8000, 0xFE);
    sim.reset_with_mode(stcjudge::ResetMode::Cpu).unwrap();
    assert_eq!(sim.latch_mode(), "MM");
    assert_eq!(sim.snapshot().board_latches_effective[0], 0xFE);
    sim.reset().unwrap();
    assert_eq!(sim.latch_mode(), "MM");
}

// V2 J6.1=VB, .2=VCC; K1 coil/LED and SP1 use VB, J3 motor uses VCC.
#[test]
fn j6_disconnects_load_power_without_erasing_control_latch() {
    let mut sim = Simulator::nop(false);
    assert!(sim.relay_on() && sim.buzzer_on() && sim.motor_on());
    sim.set_load_power(false);
    assert!(!sim.relay_on());
    assert!(!sim.buzzer_on());
    assert!(sim.motor_on());
    assert_eq!(sim.snapshot().board_latches_effective[1], 0x70);
    assert!(!sim.snapshot().relay_on && !sim.snapshot().buzzer_on);
    sim.reset().unwrap();
    assert!(!sim.relay_on() && !sim.buzzer_on());
    sim.set_load_power(true);
    assert!(sim.relay_on() && sim.buzzer_on());
}

// V2 J2: US caps 1-3/2-4; IR caps 3-5/4-6. IR electronics stay unmodelled.
#[test]
fn j2_ir_disconnects_echo_and_leaves_mcu_latches_in_control() {
    let mut sim = Simulator::nop(false);
    sim.set_distance_cm(1.7);
    sim.poke_sfr(0x90, 0xFF).unwrap();
    sim.run_us(1).unwrap();
    sim.poke_sfr(0x90, 0xFE).unwrap();
    sim.run_us(101).unwrap();
    assert_eq!(sim.peek_sfr(0x90).unwrap() & 2, 0);
    sim.set_ultrasonic_route("IR").unwrap();
    assert_eq!(sim.peek_sfr(0x90).unwrap() & 2, 2);
    sim.poke_sfr(0x90, 0xFC).unwrap();
    assert_eq!(sim.peek_sfr(0x90).unwrap() & 3, 0);
    sim.reset().unwrap();
    assert_eq!(sim.ultrasonic_route(), "IR");
    assert!(sim.set_ultrasonic_route("other").is_err());
}

#[test]
fn j2_ir_tx_changes_cannot_trigger_ultrasound() {
    let mut sim = Simulator::nop(false);
    sim.set_distance_cm(1.7);
    sim.set_ultrasonic_route("IR").unwrap();
    for value in [0xFE, 0xFF, 0xFE] {
        sim.poke_sfr(0x90, value).unwrap();
        sim.run_us(1).unwrap();
    }
    sim.run_us(101).unwrap();
    sim.set_ultrasonic_route("US").unwrap();
    assert_eq!(sim.peek_sfr(0x90).unwrap() & 2, 2);
}

#[test]
fn rb3_dial_drives_frequency_and_direct_override_survives_reset() {
    let mut sim = Simulator::nop(false);
    sim.set_rb3(0.5).unwrap();
    assert!((sim.snapshot().ne555_frequency_hz - 1414.4069).abs() < 0.02);
    assert_eq!(sim.rb3_resistance_ohm(), 5000.0);
    for invalid in [-0.01, 1.01, f32::NAN, f32::INFINITY] {
        assert!(sim.set_rb3(invalid).is_err());
    }
    assert_eq!(sim.rb3_position(), 0.5);
    sim.set_frequency_hz(2200.0);
    sim.reset().unwrap();
    assert_eq!(sim.snapshot().ne555_frequency_hz, 2200.0);
    assert_eq!(sim.rb3_position(), 0.5);
    sim.set_rb3(1.0).unwrap();
    assert!((sim.snapshot().ne555_frequency_hz - 714.20544).abs() < 0.02);
}

#[test]
fn light_and_rb2_drive_adc_channels_and_keep_direct_voltage_override() {
    let mut sim = Simulator::nop(false);
    sim.set_light(0.02).unwrap();
    sim.set_rb2(0.5).unwrap();
    assert!((sim.snapshot().analog_rd1_v - 2.857143).abs() < 0.0001);
    assert_eq!(sim.snapshot().analog_rb2_v, 2.5);
    for invalid in [-0.01, 1.01, f32::NAN, f32::INFINITY] {
        assert!(sim.set_light(invalid).is_err());
        assert!(sim.set_rb2(invalid).is_err());
    }
    sim.set_voltage("AIN1", 1.0).unwrap();
    sim.set_voltage("AIN3", 4.0).unwrap();
    sim.reset().unwrap();
    assert_eq!(sim.snapshot().analog_rd1_v, 1.0);
    assert_eq!(sim.snapshot().analog_rb2_v, 4.0);
    assert_eq!(sim.light_level(), 0.02);
    assert_eq!(sim.rb2_position(), 0.5);
    sim.set_rb2(0.0).unwrap();
    sim.set_light(0.0).unwrap();
    assert_eq!(sim.snapshot().analog_rb2_v, 0.0);
    assert!((sim.snapshot().analog_rd1_v - 0.09803922).abs() < 0.0001);
}

#[test]
fn j13_cpu_reset_resolves_the_now_open_segment_latch() {
    let mut sim = Simulator::nop(false);
    sim.set_latch_mode("MM").unwrap();
    sim.poke_xdata(0xE000, 0xC0);
    sim.poke_sfr(0xA0, 0).unwrap();
    sim.set_latch_mode("IO").unwrap();
    sim.reset_with_mode(stcjudge::ResetMode::Cpu).unwrap();
    assert_eq!(sim.snapshot().board_latches_effective[3], 0xFF);
}

#[test]
fn j13_paused_switch_updates_loads_from_real_q_immediately() {
    let mut sim = Simulator::nop(false);
    sim.set_latch_mode("MM").unwrap();
    sim.poke_sfr(0xA0, 0xA0).unwrap();
    sim.poke_sfr(0x80, 0).unwrap();
    sim.set_latch_mode("IO").unwrap();
    assert_eq!(sim.snapshot().board_latches_effective[1], 0);
    assert!(!sim.relay_on() && !sim.motor_on() && !sim.buzzer_on());
}

#[test]
fn j13_mm_wr_uses_resolved_keyboard_column_not_only_p4_latch() {
    let mut sim = Simulator::nop(false);
    sim.set_latch_mode("MM").unwrap();
    sim.poke_sfr(0xA0, 0x80).unwrap();
    sim.poke_sfr(0x80, 0xFE).unwrap();
    sim.poke_sfr(0xB0, 0xF7).unwrap();
    sim.set_key("S8", true).unwrap(); // Local V2 S9: P3.3 to P4.2/WR.
    assert_eq!(sim.peek_sfr(0xC0).unwrap() & 4, 0);
    assert_eq!(sim.snapshot().board_latches_effective[0], 0xFE);
    sim.set_key("S8", false).unwrap();
    sim.poke_sfr(0x80, 0).unwrap();
    assert_eq!(sim.snapshot().board_latches_effective[0], 0xFE);
}
