use stcjudge::{KeyId, KeyMode, RunToTarget, SignalId, Simulator};

#[test]
fn p35_matrix_s12_reads_low() -> anyhow::Result<()> {
    // SCH_V31.pdf pp.1/4: P3.5 = COL3; S12 connects COL3 to ROW4/P3.3.
    let mut sim = Simulator::nop(false);
    sim.poke_sfr(0xb0, 0xf7)?;
    sim.set_key_id(KeyId::S12, true);

    assert_eq!(sim.peek_sfr(0xb0)? & 0x20, 0, "S12 must pull P3.5 low");
    Ok(())
}

#[test]
fn p35_matrix_rows_release_and_other_columns() -> anyhow::Result<()> {
    // SCH_V31.pdf pp.1/4: COL1=P4.4, COL2=P4.2, COL3=P3.5, COL4=P3.4.
    // S12/S13 are on ROW4/P3.3 and ROW3/P3.2, respectively.
    for (key, key_row, column) in [
        (KeyId::S12, 3, 2),
        (KeyId::S13, 2, 2),
        (KeyId::S4, 3, 0),
        (KeyId::S5, 2, 0),
        (KeyId::S8, 3, 1),
        (KeyId::S9, 2, 1),
        (KeyId::S16, 3, 3),
        (KeyId::S17, 2, 3),
    ] {
        let mut sim = Simulator::nop(false);
        sim.set_key_id(key, true);
        for driven_row in 0..4 {
            sim.poke_sfr(0xb0, 0xff & !(1 << driven_row))?;
            let selected = driven_row == key_row;
            let p3_low = if selected {
                match column {
                    2 => 0x20,
                    3 => 0x10,
                    _ => 0,
                }
            } else {
                0
            };
            let p4_low = if selected {
                match column {
                    0 => 0x10,
                    1 => 0x04,
                    _ => 0,
                }
            } else {
                0
            };
            assert_eq!(sim.peek_sfr(0xb0)? & 0x30, 0x30 & !p3_low, "{key:?}");
            assert_eq!(sim.peek_sfr(0xc0)? & 0x14, 0x14 & !p4_low, "{key:?}");
        }
        sim.poke_sfr(0xb0, 0xff)?;
        assert_eq!(sim.peek_sfr(0xb0)? & 0x30, 0x30, "all rows released");
        sim.poke_sfr(0xb0, 0xff & !(1 << key_row))?;
        sim.set_key_id(key, false);
        assert_eq!(sim.peek_sfr(0xb0)? & 0x30, 0x30, "key released");
        assert_eq!(sim.peek_sfr(0xc0)? & 0x14, 0x14, "key released");
    }
    Ok(())
}

#[test]
fn p35_own_latch_low_survives_fastpath_and_both_key_modes() -> anyhow::Result<()> {
    // STC15_DS.pdf p.324, section 4.9.1: latch 0 drives low; latch 1 permits input.
    for mode in [KeyMode::Keyboard, KeyMode::Button] {
        for key in [None, Some(KeyId::S12), Some(KeyId::S4)] {
            let mut sim = Simulator::nop(false);
            sim.key_mode(mode);
            if let Some(key) = key {
                sim.set_key_id(key, true);
            }
            for latch in [0xdf, 0xff] {
                sim.poke_sfr(0xb0, latch)?;
                let expected = latch & 0x20;
                assert_eq!(sim.peek_sfr(0xb0)? & 0x20, expected, "{mode:?}/{key:?}");
                assert_eq!(sim.snapshot().port_input[3] & 0x20, expected);
                assert_eq!(
                    sim.read_run_to_target(RunToTarget::Pin { port: 3, bit: 5 }),
                    expected != 0
                );
                assert_eq!(sim.peek_sfr_latch(0xb0)?, latch);
            }
        }
    }
    Ok(())
}

#[test]
fn p35_ne555_j3_j5_wiring_truth_table() -> anyhow::Result<()> {
    // SCH_V31.pdf p.4: J3.15=NET_SIG (U15/NE555 pin 3), J3.16=SIG_OUT.
    // Page 1 routes SIG_OUT to P3.4/COL4, never to P3.5/COL3.
    // J5 1-2 selects COL1 (Keyboard); 2-3 selects GND (Button).
    for mode in [KeyMode::Keyboard, KeyMode::Button] {
        for bridge in [false, true] {
            let mut sim = Simulator::nop(false);
            sim.key_mode(mode);
            sim.set_frequency_hz(1_000.0);
            if bridge {
                sim.jumper_on(SignalId::NetSig, SignalId::SigOut)?;
            }
            // The model starts high; half a 1 kHz period later it is low.
            for ne555_high in [true, false] {
                if !ne555_high {
                    sim.run_us(500)?;
                }
                assert_eq!(sim.snapshot().ne555_level, ne555_high);
                for latch_high in [false, true] {
                    for pressed in [false, true] {
                        for (key, row) in [(KeyId::S12, 3), (KeyId::S13, 2)] {
                            for row_low in [false, true] {
                                let mut latch = if latch_high { 0xff } else { 0xdf };
                                if row_low {
                                    latch &= !(1 << row);
                                }
                                sim.poke_sfr(0xb0, latch)?;
                                sim.set_key_id(key, pressed);
                                let expected_p35 = latch_high
                                    && !(mode == KeyMode::Keyboard && pressed && row_low);
                                assert_eq!(
                                    sim.peek_sfr(0xb0)? & 0x30,
                                    (u8::from(expected_p35) << 5)
                                        | (u8::from(!bridge || ne555_high) << 4),
                                    "{mode:?}, J3={bridge}, NE555={ne555_high}, \
                                     latch={latch:#04x}, {key:?}={pressed}"
                                );
                                sim.set_key_id(key, false);
                            }
                        }
                    }
                }
            }
            sim.jumper_off(SignalId::NetSig, SignalId::SigOut)?;
            sim.poke_sfr(0xb0, 0xff)?;
            assert_eq!(sim.peek_sfr(0xb0)? & 0x30, 0x30, "J3 removed");
        }
    }
    Ok(())
}

#[test]
fn p35_timer1_samples_matrix_falling_edges() -> anyhow::Result<()> {
    // STC15_DS.pdf p.510, section 7.3.1: C/T1=1 selects P3.5/T1.
    // Page 522, section 7.3.1.4: external counter mode counts falling edges.
    // SCH_V31.pdf pp.1/4: one S12/S13 closure on its low row pulls COL3 low.
    for (key, row) in [(KeyId::S12, 3), (KeyId::S13, 2)] {
        let mut sim = Simulator::nop(false);
        sim.poke_sfr(0xb0, 0xff & !(1 << row))?;
        sim.poke_sfr(0x89, 0x40)?; // TMOD: T1 mode 0, external counter.
        sim.poke_sfr(0x88, 0x40)?; // TCON: TR1.
        sim.step_instruction()?; // Sample initial high.
        assert_eq!(sim.peek_sfr(0x8b)?, 0);
        for count in 1..=2 {
            sim.set_key_id(key, true);
            sim.step_instruction()?;
            assert_eq!(sim.peek_sfr(0x8b)?, count, "{key:?} falling edge");
            sim.step_instruction()?;
            assert_eq!(sim.peek_sfr(0x8b)?, count, "held low must not recount");
            sim.set_key_id(key, false);
            sim.step_instruction()?;
            assert_eq!(sim.peek_sfr(0x8b)?, count, "rising edge must not count");
        }
    }
    Ok(())
}
