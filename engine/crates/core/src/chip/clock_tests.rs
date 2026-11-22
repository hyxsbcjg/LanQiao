use super::*;

fn machine() -> Simulator {
    Simulator::nop(false)
}

#[test]
fn all_clks_divide_cpu_wall_time_not_system_tick_counts() {
    // 手册 p.206：SYSclk=MCLK/2^CLKS；定时器/PCA 再按各自分频计数。
    for clks in 0..8 {
        let mut sim = machine();
        sim.cpu.sfr_set(SFR_CLK_DIV, 0xf8 | clks, &mut sim.ctx);
        sim.cpu.sfr_set(
            SFR_AUXR,
            AUXR_T0_X12 | AUXR_T2_RUN | AUXR_T2_X12,
            &mut sim.ctx,
        );
        sim.cpu.sfr_set(SFR_TMOD, 0x11, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_TCON, TCON_TR0 | TCON_TR1, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_CCON, CCON_CR, &mut sim.ctx);
        sim.tick_devices(768).unwrap();
        assert_eq!(sim.sim_time_ns(), 64_000 << clks);
        assert_eq!(sim.ctx.board.cpu_cycles, 768);
        assert_eq!(sim.cpu.sfr(SFR_TH0, &sim.ctx), 3);
        assert_eq!(sim.cpu.sfr(SFR_TL0, &sim.ctx), 0);
        assert_eq!(sim.cpu.sfr(SFR_TL1, &sim.ctx), 64);
        assert_eq!(sim.cpu.sfr(SFR_T2H, &sim.ctx), 3);
        assert_eq!(sim.cpu.sfr(SFR_T2L, &sim.ctx), 0);
        assert_eq!(sim.cpu.sfr(SFR_CL, &sim.ctx), 64);
    }
}

#[test]
fn clk_div_write_uses_old_clock_then_changes_subsequent_instructions() {
    // 保留引擎原有近似指令表：MOV direct,#data=2、NOP=1 SYSclk。
    let mut sim = Simulator::from_code_with_options(
        vec![0x75, 0x97, 3, 0, 0x75, 0x97, 0, 0],
        false,
        WaveCaptureOptions::default(),
    );
    for master_ticks in [2, 10, 26, 27] {
        sim.step().unwrap();
        assert_eq!(sim.sim_time_ns(), master_ticks * NS_PER_SECOND / SYSTEM_HZ);
    }
    assert_eq!(sim.ctx.board.cpu_cycles, 6);
}

#[test]
fn uart_tx_follows_clks_but_external_rx_keeps_its_clock() {
    // 名义 100kbaud、8N1：每帧 1200 SYSclk；RX 输入源独立于 MCU 时钟。
    for clks in 0..8 {
        let mut sim = machine();
        sim.cpu.sfr_set(SFR_CLK_DIV, clks, &mut sim.ctx);
        // UART1 firmware timing is covered by uart_baud_tests; retain UART2's legacy contract.
        for uart in [&mut sim.ctx.ports.uart2] {
            uart.configure(UartConfig {
                baud_rate: 100_000,
                ..UartConfig::default()
            })
            .unwrap();
            uart.write(uart.scon_addr, SCON_REN);
            uart.write(uart.sbuf_addr, 0x55);
            uart.feed_rx_bytes(&[0x69]).unwrap();
        }
        sim.tick_devices(1199).unwrap();
        for uart in [&sim.ctx.ports.uart2] {
            assert!(uart.tx_queue.is_empty());
            assert_eq!(uart.ri_pending(), clks != 0);
        }
        sim.tick_devices(1).unwrap();
        for uart in [&sim.ctx.ports.uart2] {
            let frame = uart.tx_queue.front().unwrap();
            assert_eq!(frame.symbol, 0x55);
            assert_eq!(frame.completed_at_ns, 100_000 << clks);
            assert!(uart.ti_pending());
            assert!(uart.ri_pending());
        }
    }
}

#[test]
fn in_flight_uart_tx_rescales_only_the_untransmitted_part() {
    let mut sim = machine();
    sim.ctx
        .ports
        .uart2
        .configure(UartConfig {
            baud_rate: 100_000,
            ..UartConfig::default()
        })
        .unwrap();
    sim.ctx.ports.uart2.write(UART2_SFR_S2BUF, 0x55);
    sim.tick_devices(600).unwrap();
    sim.cpu.sfr_set(SFR_CLK_DIV, 3, &mut sim.ctx);
    sim.tick_devices(150).unwrap();
    assert!(sim.ctx.ports.uart2.tx_queue.is_empty());
    sim.cpu.sfr_set(SFR_CLK_DIV, 1, &mut sim.ctx);
    sim.tick_devices(449).unwrap();
    assert!(sim.ctx.ports.uart2.tx_queue.is_empty());
    sim.tick_devices(1).unwrap();
    let frame = sim.ctx.ports.uart2.tx_queue.front().unwrap();
    assert_eq!(frame.started_at_ns, 0);
    assert_eq!(frame.completed_at_ns, 225_000);
}

#[test]
fn clk_div_default_path_matches_explicit_zero() {
    let mut implicit = machine();
    let mut explicit = machine();
    explicit.cpu.sfr_set(SFR_CLK_DIV, 0, &mut explicit.ctx);
    for _ in 0..120 {
        implicit.step().unwrap();
        explicit.step().unwrap();
    }
    assert_eq!(implicit.sim_time_ns(), 10_000);
    assert_eq!(implicit.sim_time_ns(), explicit.sim_time_ns());
    assert_eq!(implicit.cpu.pc, explicit.cpu.pc);
    assert_eq!(implicit.ctx.board.cpu_cycles, explicit.ctx.board.cpu_cycles);
    assert_eq!(implicit.ctx.ports.generic, explicit.ctx.ports.generic);
}

#[test]
fn clk_div_scales_interrupt_entry_and_preserves_timer_divider_remainder() {
    let mut sim = machine();
    sim.cpu.sfr_set(SFR_CLK_DIV, 7, &mut sim.ctx);
    sim.cpu.sfr_set(SFR_CMOD, CMOD_ECF, &mut sim.ctx);
    sim.cpu.sfr_set(SFR_CCON, CCON_CF, &mut sim.ctx);
    sim.cpu.ie_set(IE_EA);
    sim.step().unwrap();
    assert_eq!((sim.cpu.pc, sim.sim_time_ns()), (0x3b, 32_000));

    let mut sim = machine();
    sim.cpu.sfr_set(SFR_TMOD, 1, &mut sim.ctx);
    sim.cpu.sfr_set(SFR_TCON, TCON_TR0, &mut sim.ctx);
    sim.tick_devices(11).unwrap();
    sim.cpu.sfr_set(SFR_CLK_DIV, 7, &mut sim.ctx);
    sim.step().unwrap();
    assert_eq!(
        sim.cpu.sfr(SFR_TL0, &sim.ctx),
        1,
        "动态分频不能丢失 12T 余数"
    );
    assert_eq!(sim.sim_time_ns(), 139 * NS_PER_SECOND / SYSTEM_HZ);
}
