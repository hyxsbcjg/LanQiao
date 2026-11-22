use stcjudge::{RunToEdge, RunToTarget, Simulator, WaveCaptureOptions};

#[test]
fn core_runs_with_simulated_time_only() -> anyhow::Result<()> {
    let mut sim =
        Simulator::from_code_with_options(vec![0x80, 0xfe], false, WaveCaptureOptions::default());
    sim.step()?;
    let start_ns = sim.sim_time_ns();
    assert!(start_ns > 0);

    let elapsed_ns = sim.run_to_ns(start_ns + 10_000)?;
    assert!(elapsed_ns >= 10_000);
    assert_eq!(sim.sim_time_ns(), start_ns + elapsed_ns);
    assert_eq!(
        RunToTarget::parse("P3.4")?,
        RunToTarget::Pin { port: 3, bit: 4 }
    );
    assert_eq!(RunToEdge::parse("UP")?, RunToEdge::Up);
    Ok(())
}

#[test]
fn text_hex_uses_the_existing_parser() -> anyhow::Result<()> {
    let mut sim = Simulator::from_hex_text(":0200000080FE80\n:00000001FF\n", false)?;
    sim.step_instruction()?;
    assert_eq!(sim.snapshot().pc, 0);
    assert!(Simulator::from_hex_text("无效 HEX", false).is_err());
    Ok(())
}

#[test]
fn instruction_step_executes_after_interrupt_entry() -> anyhow::Result<()> {
    let mut sim = Simulator::nop(false);
    sim.poke_sfr(0xa8, 0x81)?;
    sim.poke_sfr(0x88, 0x03)?;
    sim.step_instruction()?;
    assert_eq!(sim.snapshot().pc, 4, "应执行 INT0 入口 0003H 的 NOP");

    let mut legacy = Simulator::nop(false);
    legacy.poke_sfr(0xa8, 0x81)?;
    legacy.poke_sfr(0x88, 0x03)?;
    legacy.step()?;
    assert_eq!(legacy.snapshot().pc, 3, "原有 step 仍仅受理中断入口");
    Ok(())
}
