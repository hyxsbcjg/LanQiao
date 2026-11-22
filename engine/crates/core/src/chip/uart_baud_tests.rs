use super::*;

#[test]
fn irc_uart_same_reload_scales_baud_and_tx_timing_with_sysclk() {
    // STC15_DS.pdf pp.609,625–626: T2/1T baud=SYSclk/(4*(65536-reload)).
    // CN pp.27–28: SYSclk=IRC/2^CLKS; p.610: TI at stop-bit start.
    for hz in [5_000_000, 11_059_200, 12_000_000, 35_000_000] {
        for clks in 0..8 {
            let mut sim = Simulator::nop(false);
            sim.set_irc_hz(hz).unwrap();
            for (reg, value) in [
                (SFR_CLK_DIV, clks),
                (SFR_T2H, 0xfe),
                (SFR_T2L, 0xc7),
                (SFR_AUXR, 0x15),
                (SFR_SCON, 0x50),
                (SFR_SBUF, 0x55),
            ] {
                sim.poke_sfr(reg, value).unwrap();
            }
            let divider = 1 << clks;
            let sysclk = hz / divider;
            assert_eq!(sim.uart1_firmware_baud(), Some(sysclk as f64 / 1252.0));
            assert_eq!(sim.uart_terminal_baud(), 9600);
            let bit_ns = (1252 * NS_PER_SECOND).div_ceil(sysclk);
            let cycles = (9 * bit_ns * hz).div_ceil(NS_PER_SECOND * divider) as u32;
            sim.tick_devices(cycles - 1).unwrap();
            assert!(!sim.ctx.ports.uart1.ti_pending(), "hz={hz}, clks={clks}");
            sim.tick_devices(1).unwrap();
            assert!(sim.ctx.ports.uart1.ti_pending(), "hz={hz}, clks={clks}");
        }
    }
}

#[test]
fn terminal_mismatch_does_not_change_firmware_tx_timing_or_preserve_text() {
    let mut matched = Simulator::from_hex_text(
        include_str!("../../../../samples/uart_periodic_tx/prj/Objects/uart_periodic_tx.hex"),
        false,
    )
    .unwrap();
    let mut sim = Simulator::from_hex_text(
        include_str!("../../../../samples/uart_periodic_tx/prj/Objects/uart_periodic_tx.hex"),
        false,
    )
    .unwrap();
    sim.configure_uart1(UartConfig {
        baud_rate: 4800,
        ..Default::default()
    })
    .unwrap();
    matched.run_ms(500).unwrap();
    sim.run_ms(500).unwrap();
    let correct = matched.uart_take_raw();
    assert!(!correct.is_empty());
    assert_ne!(sim.uart_take_raw(), correct);
}

#[test]
fn baud_uses_timer_reload_shadows_and_only_applicable_smod() {
    // STC15_DS.pdf pp.609,619,625–626, SYSclk=12MHz.
    for (auxr, tmod, high, low, pcon, ticks) in [
        (0, 0, 0xff, 0xe6, 0, 26 * 12 * 4),
        (0x40, 0, 0xfe, 0xc7, 0, 313 * 4),
        (0x40, 0, 0xfe, 0xc7, 0x80, 313 * 4),
        (0, 0x20, 0xfd, 0, 0, 3 * 12 * 32),
        (0, 0x20, 0xfd, 0, 0x80, 3 * 12 * 16),
        (0x40, 0x20, 0xd9, 0, 0, 39 * 32),
        (0x11, 0, 0xff, 0xe6, 0x80, 26 * 12 * 4),
        (0x15, 0, 0xfe, 0xc7, 0, 313 * 4),
    ] {
        let mut sim = Simulator::nop(false);
        for (reg, value) in [
            (SFR_SCON, 0x50),
            (SFR_PCON, pcon),
            (SFR_TMOD, tmod),
            (SFR_TH1, high),
            (SFR_TL1, low),
            (SFR_T2H, high),
            (SFR_T2L, low),
            (SFR_AUXR, auxr),
            (SFR_TCON, TCON_TR1),
        ] {
            sim.poke_sfr(reg, value).unwrap();
        }
        assert_eq!(sim.uart1_bit_ticks(), Some(ticks));
        sim.tick_devices(1300).unwrap();
        assert_eq!(sim.uart1_bit_ticks(), Some(ticks));
        assert_eq!(sim.uart1_firmware_baud(), Some(12_000_000.0 / ticks as f64));
    }
    let mut sim = Simulator::nop(false);
    assert_eq!(sim.uart1_firmware_baud(), None);
    sim.poke_sfr(SFR_SCON, 0x50).unwrap();
    assert_eq!(sim.uart1_firmware_baud(), None); // Stopped timer.
    for (scon, auxr, pcon, ticks) in [
        (0x10, 0, 0, 12),
        (0x10, 0x20, 0, 2),
        (0x90, 0, 0, 64),
        (0x90, 0, 0x80, 32),
    ] {
        sim.poke_sfr(SFR_SCON, scon).unwrap();
        sim.poke_sfr(SFR_AUXR, auxr).unwrap();
        sim.poke_sfr(SFR_PCON, pcon).unwrap();
        assert_eq!(sim.uart1_bit_ticks(), Some(ticks));
    }
}

fn decode(bytes: &[u8], sender: u32, receiver: u32) -> Vec<(u16, bool)> {
    let config = UartConfig {
        baud_rate: sender,
        ..Default::default()
    };
    let bit_ns = UartConfig {
        baud_rate: receiver,
        ..Default::default()
    }
    .bit_ns();
    let mut decoder = Receiver::default();
    let mut out = Vec::new();
    let mut now = 0;
    for byte in bytes {
        let mut frame = UartFrame::new(u16::from(*byte), config, now);
        let end = frame.completed_at_ns;
        sample(&mut frame, &mut decoder, now, end, bit_ns, 8, &mut out);
        now = end;
    }
    decoder.segment(true, now, now + 12 * bit_ns, bit_ns, 8, &mut out);
    out.into_iter().map(|r| (r.timed.symbol, r.stop)).collect()
}

#[test]
fn receiver_exact_and_two_percent_in_both_directions_for_all_bytes() {
    let bytes = (0..=255).collect::<Vec<u8>>();
    for baud in [9408, 9600, 9792] {
        let expected = bytes
            .iter()
            .map(|b| (u16::from(*b), true))
            .collect::<Vec<_>>();
        assert_eq!(decode(&bytes, 9600, baud), expected);
        assert_eq!(decode(&bytes, baud, 9600), expected);
    }
}

#[test]
fn receiver_mismatch_hunts_data_edges_and_checks_stop_bits() {
    // Independent hand sampling of 0x55: L H L H L H L H L H.
    // Rounded bit periods 104167ns/208333ns sample just before sender boundaries.
    assert_eq!(decode(&[0x55], 9600, 4800), vec![(0xf0, true)]);
    assert_eq!(
        decode(&[0x55], 4800, 9600),
        vec![(0x66, false), (0xe6, true)]
    );
    // A 12x receiver finishes inside each low sender bit; a 12x sender's 00 ends before data sampling.
    assert_eq!(decode(&[0x55], 9600, 115200), vec![(0, false); 5]);
    assert_eq!(decode(&[0], 115200, 9600), vec![(0xff, true)]);
    for (sender, receiver) in [(9600, 115200), (115200, 9600)] {
        let a = decode(b"OKERROR", sender, receiver);
        assert!(!a.is_empty());
        assert_ne!(a.iter().map(|v| v.0 as u8).collect::<Vec<_>>(), b"OKERROR");
        assert_eq!(a, decode(b"OKERROR", sender, receiver));
    }
}

#[test]
fn uart1_ti_uses_firmware_stop_start_not_terminal_frame_time() {
    // p.610: TI at stop-bit start (9 bits in mode1), RI at stop midpoint.
    for clks in 0..8 {
        let mut sim = Simulator::nop(false);
        for (reg, value) in [
            (SFR_T2H, 0xff),
            (SFR_T2L, 0xe2),
            (SFR_AUXR, 0x15),
            (SFR_SCON, 0x50),
            (SFR_CLK_DIV, clks),
            (SFR_SBUF, 0x55),
        ] {
            sim.poke_sfr(reg, value).unwrap();
        }
        sim.set_uart_terminal_baud(4800).unwrap();
        sim.tick_devices(1079).unwrap();
        assert!(!sim.ctx.ports.uart1.ti_pending());
        sim.tick_devices(1).unwrap();
        assert!(sim.ctx.ports.uart1.ti_pending());
        assert_eq!(sim.sim_time_ns(), 90_000 << clks);
        assert_eq!(
            sim.uart1_firmware_baud(),
            Some(100_000.0 / (1 << clks) as f64)
        );
    }
}

#[test]
fn mode0_completes_eight_shift_clocks_at_fixed_rate() {
    // pp.615–616: 8 synchronous shift clocks, no start/stop bits.
    let mut sim = Simulator::nop(false);
    sim.poke_sfr(SFR_SCON, 0).unwrap();
    sim.poke_sfr(SFR_SBUF, 0x55).unwrap();
    sim.tick_devices(95).unwrap();
    assert!(!sim.ctx.ports.uart1.ti_pending());
    sim.tick_devices(1).unwrap();
    assert!(sim.ctx.ports.uart1.ti_pending());
}

#[test]
fn stopped_baud_timer_holds_tx_phase_until_restart() {
    let mut sim = Simulator::nop(false);
    for (reg, value) in [
        (SFR_T2H, 0xff),
        (SFR_T2L, 0xe2),
        (SFR_AUXR, 0x15),
        (SFR_SCON, 0x50),
        (SFR_SBUF, 0x55),
    ] {
        sim.poke_sfr(reg, value).unwrap();
    }
    sim.tick_devices(600).unwrap();
    sim.poke_sfr(SFR_AUXR, 0x05).unwrap();
    sim.tick_devices(1200).unwrap();
    assert!(!sim.ctx.ports.uart1.ti_pending());
    sim.poke_sfr(SFR_AUXR, 0x15).unwrap();
    sim.tick_devices(479).unwrap();
    assert!(!sim.ctx.ports.uart1.ti_pending());
    sim.tick_devices(1).unwrap();
    assert!(sim.ctx.ports.uart1.ti_pending());
}

fn rx_machine() -> Simulator {
    let mut sim = Simulator::nop(false);
    for (reg, value) in [
        (SFR_T2H, 0xff),
        (SFR_T2L, 0xe2),
        (SFR_AUXR, 0x15),
        (SFR_SCON, 0x50),
    ] {
        sim.poke_sfr(reg, value).unwrap();
    }
    sim.set_uart_terminal_baud(100_000).unwrap();
    sim
}

#[test]
fn ri_is_visible_at_exact_stop_midpoint() {
    let mut sim = rx_machine();
    sim.uart_write(b"A").unwrap();
    sim.tick_devices(1139).unwrap();
    assert!(!sim.ctx.ports.uart1.ri_pending());
    sim.tick_devices(1).unwrap();
    assert_eq!(sim.sim_time_ns(), 95_000);
    assert!(sim.ctx.ports.uart1.ri_pending());
    assert_eq!(sim.peek_sfr(SFR_SBUF).unwrap(), b'A');
}

#[test]
fn switching_through_sync_mode_does_not_wedge_external_rx() {
    let mut sim = rx_machine();
    sim.uart_write(b"U").unwrap();
    sim.tick_devices(600).unwrap();
    sim.poke_sfr(SFR_SCON, 0).unwrap();
    sim.tick_devices(2400).unwrap();
    sim.poke_sfr(SFR_SCON, 0x50).unwrap();
    sim.uart_write(b"A").unwrap();
    sim.tick_devices(2400).unwrap();
    assert!(sim.ctx.ports.uart1.ri_pending());
    assert_eq!(sim.peek_sfr(SFR_SBUF).unwrap(), b'A');
}

#[test]
fn rx_sampling_tracks_mid_frame_reload_change() {
    let mut sim = rx_machine();
    sim.uart_write(b"U").unwrap();
    sim.tick_devices(60).unwrap(); // Start midpoint, 5us.
    sim.poke_sfr(SFR_T2L, 0xc4).unwrap(); // Double bit period, preserving remaining phase.
    sim.tick_devices(2159).unwrap();
    assert!(!sim.ctx.ports.uart1.ri_pending());
    sim.tick_devices(1).unwrap();
    assert_eq!(sim.sim_time_ns(), 185_000);
    assert!(sim.ctx.ports.uart1.ri_pending());
    assert_eq!(sim.peek_sfr(SFR_SBUF).unwrap(), 0xf0);
}

#[test]
fn rx_framing_rb8_sm2_and_unread_sbuf_follow_mode1_rules() {
    // pp.610,617: RB8=stop bit, SM2 filters low stops, RI=1 prevents overwrite.
    let mut sim = rx_machine();
    sim.set_uart_terminal_baud(50_000).unwrap();
    sim.uart_write(b"U").unwrap();
    sim.tick_devices(1140).unwrap();
    assert_eq!(sim.peek_sfr(SFR_SBUF).unwrap(), 0x66);
    assert_eq!(
        sim.peek_sfr(SFR_SCON).unwrap() & (SCON_RI | SCON_RB8),
        SCON_RI
    );
    sim.tick_devices(2400).unwrap();
    assert_eq!(sim.peek_sfr(SFR_SBUF).unwrap(), 0x66);

    let mut sim = rx_machine();
    sim.poke_sfr(SFR_SCON, 0x70).unwrap();
    sim.set_uart_terminal_baud(50_000).unwrap();
    sim.uart_write(b"U").unwrap();
    sim.tick_devices(1140).unwrap();
    assert!(!sim.ctx.ports.uart1.ri_pending());
    sim.tick_devices(2400).unwrap();
    assert_eq!(sim.peek_sfr(SFR_SBUF).unwrap(), 0xe6);
    assert_eq!(
        sim.peek_sfr(SFR_SCON).unwrap() & (SCON_RI | SCON_RB8),
        SCON_RI | SCON_RB8
    );
}
