use super::*;

fn machine(cmod: u8, counter: u16) -> Simulator {
    let mut sim = Simulator::nop(false);
    sim.cpu.sfr_set(SFR_CMOD, cmod, &mut sim.ctx);
    sim.cpu.sfr_set(SFR_CH, (counter >> 8) as u8, &mut sim.ctx);
    sim.cpu.sfr_set(SFR_CL, counter as u8, &mut sim.ctx);
    sim.cpu.sfr_set(SFR_CCON, CCON_CR, &mut sim.ctx);
    sim
}

fn counter(sim: &Simulator) -> u16 {
    u16::from_be_bytes([sim.cpu.sfr(SFR_CH, &sim.ctx), sim.cpu.sfr(SFR_CL, &sim.ctx)])
}

#[test]
fn pca_system_sources_overflow_at_manual_divisors() {
    // 手册 p.784：从 FFFEh 开始，2×分频数个 NOP 后溢出；覆盖前/中/后三点。
    for (source, divisor) in [(0, 12), (1, 2), (4, 1), (5, 4), (6, 6), (7, 8)] {
        for delta in [-1, 0, 1] {
            let mut sim = machine((source << 1) | CMOD_ECF, 0xfffe);
            let ticks = (2 * divisor + delta) as u32;
            for _ in 0..ticks {
                sim.step().unwrap();
            }
            assert_eq!(
                counter(&sim),
                0xfffe_u16.wrapping_add((ticks / divisor as u32) as u16)
            );
            assert_eq!(sim.cpu.sfr(SFR_CCON, &sim.ctx) & CCON_CF != 0, delta >= 0);
            assert_eq!(
                sim.sim_time_ns(),
                u64::from(ticks) * NS_PER_SECOND / SYSTEM_HZ
            );
        }
    }
}

#[test]
fn pca_t0_clock_counts_each_overflow_not_the_tf0_level() {
    for mode in 0..3 {
        let mut sim = machine(2 << 1, 0);
        sim.cpu.sfr_set(SFR_AUXR, AUXR_T0_X12, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_TMOD, mode, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_TH0, 0xff, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_TL0, 0xff, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_TCON, TCON_TR0 | TCON_TF0, &mut sim.ctx);
        sim.tick_devices(7).unwrap();
        assert_eq!(counter(&sim), if mode == 1 { 1 } else { 7 });
        sim.tick_devices(5).unwrap();
        assert_eq!(counter(&sim), if mode == 1 { 1 } else { 12 });
    }
}

#[test]
fn pca_eci_counts_p1_2_falling_edges_and_respects_cr() {
    let code = vec![0xc2, 0x92, 0xd2, 0x92, 0xc2, 0x92];
    let mut sim = Simulator::from_code_with_options(code, false, WaveCaptureOptions::default());
    sim.cpu.sfr_set(SFR_CMOD, 3 << 1, &mut sim.ctx);
    sim.cpu.sfr_set(SFR_CCON, CCON_CR, &mut sim.ctx);
    for expected in [1, 1, 2] {
        sim.step().unwrap();
        assert_eq!(counter(&sim), expected);
    }
    sim.cpu.sfr_set(SFR_CCON, 0, &mut sim.ctx);
    sim.cpu.pc = 2;
    sim.step().unwrap();
    sim.step().unwrap();
    assert_eq!(counter(&sim), 2);
}

#[test]
fn pca_cr_and_cidl_control_all_clock_sources() {
    for source in 0..8 {
        let mut sim = machine((source << 1) | CMOD_CIDL, 0x1234);
        sim.cpu.sfr_set(SFR_AUXR, AUXR_T0_X12, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_TMOD, 2, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_TH0, 0xff, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_TL0, 0xff, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_TCON, TCON_TR0, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_PCON, PCON_IDL, &mut sim.ctx);
        sim.tick_devices(24).unwrap();
        sim.ctx
            .ports
            .timers
            .sample_pca_eci(0xff, 0xfb, &mut sim.ctx.ports.generic);
        assert_eq!(counter(&sim), 0x1234);
        sim.cpu.sfr_set(SFR_PCON, 0, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_CCON, 0, &mut sim.ctx);
        sim.tick_devices(24).unwrap();
        sim.ctx
            .ports
            .timers
            .sample_pca_eci(0xff, 0xfb, &mut sim.ctx.ports.generic);
        assert_eq!(counter(&sim), 0x1234);
    }
}

#[test]
fn pca_interrupt_gates_vector_software_clear_and_reti_reassertion() {
    let mut sim = machine(0, 0xffff);
    sim.tick_devices(12).unwrap();
    sim.cpu.ie_set(IE_EA);
    sim.step().unwrap();
    assert_eq!(sim.cpu.pc, 1, "ECF=0 时 CF 不产生中断");
    sim.cpu.sfr_set(SFR_CMOD, CMOD_ECF, &mut sim.ctx);
    sim.cpu.ie_set(0);
    sim.step().unwrap();
    assert_eq!(sim.cpu.pc, 2, "EA=0 时 CF 保持挂起");
    sim.cpu.ie_set(IE_EA);
    sim.step().unwrap();
    assert_eq!((sim.cpu.pc, sim.cpu.sp()), (0x3b, 9));
    assert_ne!(
        sim.cpu.sfr(SFR_CCON, &sim.ctx) & CCON_CF,
        0,
        "进入 ISR 不自动清 CF"
    );
    sim.ctx.code.code.resize(0x40, 0);
    sim.ctx.code.code[0x3b] = 0x32;
    sim.step().unwrap();
    assert_eq!((sim.cpu.pc, sim.cpu.sp()), (2, 7));
    sim.step().unwrap();
    assert_eq!(sim.cpu.pc, 3);
    sim.step().unwrap();
    assert_eq!(sim.cpu.pc, 0x3b, "软件未清 CF 应重复请求");
    sim.ctx.code.code[0x3b..0x3e].copy_from_slice(&[0xc2, 0xdf, 0x32]);
    sim.step().unwrap();
    assert_eq!(sim.cpu.sfr(SFR_CCON, &sim.ctx) & CCON_CF, 0);
    sim.step().unwrap();
    sim.step().unwrap();
    sim.step().unwrap();
    assert_eq!(sim.cpu.pc, 5);
}

#[test]
fn pca_ppca_and_vector_order_join_unified_arbitration() {
    for (ip, expected) in [(0, 0x0b), (IP_PPCA, 0x3b), (IP_PPCA | IE_ET0, 0x0b)] {
        let mut sim = machine(CMOD_ECF, 0);
        sim.cpu.ie_set(IE_EA | IE_ET0);
        sim.cpu.ip_set(ip);
        sim.cpu.sfr_set(SFR_CCON, CCON_CF, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_TCON, TCON_TF0, &mut sim.ctx);
        sim.step().unwrap();
        assert_eq!(sim.cpu.pc, expected);
    }
    let mut sim = machine(CMOD_ECF, 0);
    sim.cpu.ie_set(IE_EA);
    sim.cpu.sfr_set(SFR_IE2, IE2_ES2, &mut sim.ctx);
    sim.cpu.sfr_set(SFR_S2CON, S2CON_TI, &mut sim.ctx);
    sim.cpu.sfr_set(SFR_CCON, CCON_CF, &mut sim.ctx);
    sim.step().unwrap();
    assert_eq!(sim.cpu.pc, 0x3b, "同级 PCA 在 UART2 之前");
}

#[test]
fn pca_idle_continues_or_stops_and_interrupt_wakes_cpu() {
    for cidl in [0, CMOD_CIDL] {
        let mut sim = machine((4 << 1) | CMOD_ECF | cidl, 0xfffe);
        sim.cpu.ie_set(IE_EA);
        sim.cpu.sfr_set(SFR_PCON, PCON_IDL, &mut sim.ctx);
        sim.step().unwrap();
        sim.step().unwrap();
        assert_eq!(sim.cpu.pc, 0, "空闲期间不执行主程序");
        if cidl == 0 {
            sim.step().unwrap();
            assert_eq!(sim.cpu.pc, 0x3b);
            assert_eq!(sim.cpu.sfr(SFR_PCON, &sim.ctx) & PCON_IDL, 0);
        } else {
            assert_eq!(counter(&sim), 0xfffe);
            sim.cpu.sfr_set(SFR_CCON, CCON_CR | CCON_CF, &mut sim.ctx);
            sim.step().unwrap();
            assert_eq!(sim.cpu.pc, 0x3b, "已挂起中断仍可唤醒 CIDL 停计数的 CPU");
            assert_eq!(sim.cpu.sfr(SFR_PCON, &sim.ctx) & PCON_IDL, 0);
        }
    }
}

#[test]
fn pca_and_timer_interrupts_preempt_in_both_directions() {
    for pca_high in [true, false] {
        let mut sim = machine(CMOD_ECF, 0);
        sim.ctx.code.code.resize(0x40, 0);
        sim.ctx.code.code[0x0b] = 0x32;
        sim.ctx.code.code[0x3b] = 0x32;
        sim.cpu.ie_set(IE_EA | IE_ET0);
        sim.cpu.ip_set(if pca_high { IP_PPCA } else { IE_ET0 });
        let (first, second) = if pca_high { (0x0b, 0x3b) } else { (0x3b, 0x0b) };
        if pca_high {
            sim.cpu.sfr_set(SFR_TCON, TCON_TF0, &mut sim.ctx);
        } else {
            sim.cpu.sfr_set(SFR_CCON, CCON_CF, &mut sim.ctx);
        }
        sim.step().unwrap();
        assert_eq!(sim.cpu.pc, first);
        sim.cpu.sfr_set(SFR_TCON, TCON_TF0, &mut sim.ctx);
        sim.cpu.sfr_set(SFR_CCON, CCON_CF, &mut sim.ctx);
        sim.step().unwrap();
        assert_eq!((sim.cpu.pc, sim.cpu.sp()), (second, 11));
        sim.step().unwrap();
        assert_eq!((sim.cpu.pc, sim.cpu.sp()), (first, 9));
    }
}

#[test]
fn pca_external_timer0_overflows_are_independent_of_cpu_clock() {
    let mut sim = machine(2 << 1, 0);
    sim.cpu.sfr_set(SFR_TMOD, TMOD_C_T0 | 2, &mut sim.ctx);
    sim.cpu.sfr_set(SFR_TH0, 0xff, &mut sim.ctx);
    sim.cpu.sfr_set(SFR_TL0, 0xff, &mut sim.ctx);
    sim.cpu.sfr_set(SFR_TCON, TCON_TR0, &mut sim.ctx);
    sim.set_frequency_hz(1_000_000.0);
    sim.jumper_on(SignalId::NetSig, SignalId::SigOut).unwrap();
    sim.tick_devices(120).unwrap();
    assert_eq!(counter(&sim), 10, "10us 内的 10 个下降沿各使 T0 溢出一次");
}

#[test]
fn pca_batch_wraps_and_ccap_registers_remain_unimplemented() {
    let mut sim = machine(4 << 1, 0);
    sim.tick_devices(2 * 65536 + 1).unwrap();
    assert_eq!(counter(&sim), 1);
    assert_ne!(sim.cpu.sfr(SFR_CCON, &sim.ctx) & CCON_CF, 0);
    sim.cpu.sfr_set(SFR_CCON, CCON_CR | 7, &mut sim.ctx);
    for addr in [SFR_CCAPM0, SFR_CCAPM1, SFR_CCAPM2, SFR_CCAP0H, SFR_CCAP0L] {
        sim.cpu.sfr_set(addr, 0xff, &mut sim.ctx);
        assert_eq!(sim.cpu.sfr(addr, &sim.ctx), 0xff);
    }
    sim.cpu.ie_set(IE_EA);
    sim.cpu.sfr_set(SFR_CMOD, (4 << 1) | CMOD_ECF, &mut sim.ctx);
    sim.step().unwrap();
    assert_eq!(
        sim.cpu.pc, 1,
        "保留原有 CCAP/CCF 存储行为，不生成捕获比较中断"
    );
}
