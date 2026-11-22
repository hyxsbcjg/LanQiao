use super::*;

#[test]
fn timer_vectors_mode0_running_writes_only_update_reload() {
    // 手册 pp.493、510、542：运行中的自动重装载值与当前计数器分离。
    for timer in 0..3 {
        let mut sim = Simulator::nop(false);
        let (high, low) = if timer == 2 {
            (SFR_T2H, SFR_T2L)
        } else {
            (SFR_TH0 + timer, SFR_TL0 + timer)
        };
        sim.cpu.sfr_set(high, 0xff, &mut sim.ctx);
        sim.cpu.sfr_set(low, 0xfe, &mut sim.ctx);
        if timer == 2 {
            sim.cpu
                .sfr_set(SFR_AUXR, AUXR_T2_RUN | AUXR_T2_X12, &mut sim.ctx);
        } else {
            sim.cpu
                .sfr_set(SFR_AUXR, AUXR_T0_X12 | AUXR_T1_X12, &mut sim.ctx);
            sim.cpu
                .sfr_set(SFR_TCON, TCON_TR0 << (timer * 2), &mut sim.ctx);
        }
        sim.step().unwrap();
        sim.step().unwrap();
        assert_eq!(sim.cpu.sfr(low, &sim.ctx), 0xfe);
        sim.cpu.sfr_set(low, 0xfc, &mut sim.ctx);
        assert_eq!(
            sim.cpu.sfr(low, &sim.ctx),
            0xfe,
            "T{timer} 运行写不能改当前计数"
        );
        sim.step().unwrap();
        sim.step().unwrap();
        assert_eq!(sim.cpu.sfr(low, &sim.ctx), 0xfc);
        sim.tick_devices(4).unwrap();
        assert_eq!(sim.cpu.sfr(low, &sim.ctx), 0xfc);
        sim.cpu.sfr_set(high, 0x12, &mut sim.ctx);
        assert_eq!(sim.cpu.sfr(high, &sim.ctx), 0xff);
    }
}

#[test]
fn timer_vectors_mode1_mode2_and_12t_remainder() {
    for timer in 0..2 {
        for mode in [1, 2] {
            let mut sim = Simulator::nop(false);
            sim.cpu
                .sfr_set(SFR_AUXR, AUXR_T0_X12 | AUXR_T1_X12, &mut sim.ctx);
            sim.cpu.sfr_set(SFR_TMOD, mode << (timer * 4), &mut sim.ctx);
            sim.cpu.sfr_set(
                SFR_TH0 + timer,
                if mode == 1 { 0xff } else { 0x80 },
                &mut sim.ctx,
            );
            sim.cpu.sfr_set(SFR_TL0 + timer, 0xfe, &mut sim.ctx);
            sim.cpu
                .sfr_set(SFR_TCON, TCON_TR0 << (timer * 2), &mut sim.ctx);
            sim.step().unwrap();
            sim.step().unwrap();
            assert_eq!(
                sim.cpu.sfr(SFR_TL0 + timer, &sim.ctx),
                if mode == 1 { 0 } else { 0x80 }
            );
            assert_eq!(
                sim.cpu.sfr(SFR_TH0 + timer, &sim.ctx),
                if mode == 1 { 0 } else { 0x80 }
            );
            assert_ne!(
                sim.cpu.sfr(SFR_TCON, &sim.ctx) & (TCON_TF0 << (timer * 2)),
                0
            );
        }
    }
    let mut sim = Simulator::nop(false);
    sim.cpu.sfr_set(SFR_TMOD, 1, &mut sim.ctx);
    sim.cpu.sfr_set(SFR_TCON, TCON_TR0, &mut sim.ctx);
    for _ in 0..11 {
        sim.step().unwrap();
    }
    assert_eq!(sim.cpu.sfr(SFR_TL0, &sim.ctx), 0);
    sim.step().unwrap();
    assert_eq!(sim.cpu.sfr(SFR_TL0, &sim.ctx), 1);
}
