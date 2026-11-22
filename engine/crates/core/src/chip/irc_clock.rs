use super::*;

impl Simulator {
    /// Selected ISP main clock, before CLK_DIV (STC15 CN PDF pp.286-287).
    pub fn irc_hz(&self) -> u64 {
        self.irc_hz
    }

    /// Post-CLK_DIV clock shared by CPU and synchronous peripherals (CN PDF pp.27-28).
    pub fn sysclk_hz(&self) -> u64 {
        self.irc_hz() / self.clock_divider()
    }

    /// Re-download with IAP15F2K61S2's 5-35 MHz IRC selection; retain nonvolatile state.
    pub fn set_irc_hz(&mut self, hz: u64) -> Result<()> {
        if !(5_000_000..=35_000_000).contains(&hz) {
            bail!("IRC frequency must be within 5000000..=35000000 Hz");
        }
        self.reset_with_mode(ResetMode::Power)?;
        self.irc_hz = hz;
        // Fractional ns belong to the previous clock domain, not the new download.
        self.ctx.board.sim_time_ns_remainder = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn irc_timer_overflow_and_pca_scale_with_main_clock_and_clk_div() {
        // STC15 CN PDF pp.27-28: SYSclk=MCLK/2^CLKS; EN pp.433-436,
        // 442-443, 784-785: T0/T1/T2 1T/12T and PCA SYSclk/12.
        for hz in [5_000_000, 11_059_200, 12_000_000, 35_000_000] {
            for clks in 0..8 {
                let mut sim = Simulator::nop(false);
                sim.set_irc_hz(hz).unwrap();
                sim.poke_sfr(SFR_CLK_DIV, clks).unwrap();
                assert_eq!(sim.irc_hz(), hz);
                assert_eq!(sim.sysclk_hz(), hz / (1 << clks));
                sim.poke_sfr(SFR_TMOD, 0x11).unwrap();
                sim.poke_sfr(SFR_AUXR, AUXR_T2_RUN | AUXR_T2_X12).unwrap();
                sim.poke_sfr(SFR_TH0, 0xfc).unwrap();
                sim.poke_sfr(SFR_TL0, 0x18).unwrap(); // 1000 * 12 SYSclk.
                sim.poke_sfr(SFR_TCON, TCON_TR0 | TCON_TR1).unwrap();
                sim.poke_sfr(SFR_CCON, CCON_CR).unwrap();
                sim.tick_devices(11_999).unwrap();
                assert_eq!(sim.peek_sfr(SFR_TCON).unwrap() & TCON_TF0, 0);
                sim.tick_devices(1).unwrap();
                assert_ne!(sim.peek_sfr(SFR_TCON).unwrap() & TCON_TF0, 0);
                assert_eq!(sim.peek_sfr(SFR_TH1).unwrap(), 3);
                assert_eq!(sim.peek_sfr(SFR_TL1).unwrap(), 0xe8);
                assert_eq!(sim.peek_sfr(SFR_T2H).unwrap(), 0x2e);
                assert_eq!(sim.peek_sfr(SFR_T2L).unwrap(), 0xe0);
                assert_eq!(sim.peek_sfr(SFR_CH).unwrap(), 3);
                assert_eq!(sim.peek_sfr(SFR_CL).unwrap(), 0xe8);
                assert_eq!(sim.sim_time_ns(), (12_000 * NS_PER_SECOND * (1 << clks)) / hz);
            }
        }
    }

    #[test]
    fn irc_fractional_nanoseconds_do_not_accumulate_rounding_drift() {
        let mut sim = Simulator::nop(false);
        sim.set_irc_hz(35_000_000).unwrap();
        for _ in 0..35 {
            sim.step().unwrap();
        }
        assert_eq!(sim.sim_time_ns(), 1_000);
        sim.poke_sfr(SFR_CLK_DIV, 3).unwrap();
        for _ in 0..35 {
            sim.step().unwrap();
        }
        assert_eq!(sim.sim_time_ns(), 9_000);
    }

    #[test]
    fn irc_rejects_invalid_values_before_changing_any_state() {
        let mut sim = Simulator::nop(false);
        sim.poke_iram(0x40, 0xa5);
        sim.step().unwrap();
        let before = sim.snapshot_text();
        for hz in [0, 4_999_999, 35_000_001, u64::MAX] {
            assert!(sim.set_irc_hz(hz).is_err());
            assert_eq!(sim.irc_hz(), 12_000_000);
            assert_eq!(sim.peek_iram(0x40), 0xa5);
            assert_eq!(sim.snapshot_text(), before);
        }
    }

    #[test]
    fn irc_download_power_cycles_ram_and_latches_but_keeps_program_and_eeprom() {
        let mut sim = Simulator::from_code_with_options(
            vec![0x75, 0x40, 0x5a, 0x80, 0xfe], false, WaveCaptureOptions::default(),
        );
        sim.set_latch_mode("MM").unwrap();
        sim.poke_xdata(0x8000, 0xfe);
        sim.poke_xdata(0x0123, 0xa5);
        sim.set_eeprom_byte(7, 0x69);
        sim.step().unwrap();
        assert_eq!(sim.peek_iram(0x40), 0x5a);
        sim.poke_sfr(SFR_CLK_DIV, 7).unwrap();
        let time = sim.sim_time_ns();
        sim.set_irc_hz(35_000_000).unwrap();
        assert_eq!(sim.sim_time_ns(), time);
        assert_eq!(sim.snapshot().pc, 0);
        assert_eq!(sim.peek_iram(0x40), 0);
        assert_eq!(sim.peek_xdata(0x0123), 0);
        assert_eq!(sim.snapshot().board_latches_effective, BOARD_POWER_ON_LATCHES);
        assert_eq!(sim.peek_sfr(SFR_CLK_DIV).unwrap(), 0);
        assert_eq!(sim.eeprom_byte(7), 0x69);
        sim.step().unwrap();
        assert_eq!(sim.peek_iram(0x40), 0x5a);
        assert_eq!(sim.sim_time_ns() - time, 2 * NS_PER_SECOND / 35_000_000);
        for mode in [ResetMode::Cpu, ResetMode::Power] {
            sim.reset_with_mode(mode).unwrap();
            assert_eq!(sim.irc_hz(), 35_000_000);
        }
        sim.set_irc_hz(35_000_000).unwrap(); // Same-frequency download still resets.
        assert_eq!(sim.peek_iram(0x40), 0);
    }
}
