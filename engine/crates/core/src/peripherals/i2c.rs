use super::{AnalogInputs, At24c02, Pcf8591};

#[derive(Debug, Default)]
pub(crate) struct I2cBus;

impl I2cBus {
    pub(crate) fn line_levels(
        &self,
        master_scl_high: bool,
        master_sda_high: bool,
        slave_scl_low: bool,
        slave_sda_low: bool,
    ) -> (bool, bool) {
        (
            master_scl_high && !slave_scl_low,
            master_sda_high && !slave_sda_low,
        )
    }

    pub(crate) fn sample(
        &mut self,
        time_ns: u64,
        master_scl_high: bool,
        master_sda_high: bool,
        analog: &AnalogInputs,
        pcf8591: &mut Pcf8591,
        at24c02: &mut At24c02,
    ) {
        let (slave_scl_low, slave_sda_low) = self.slave_drives_low(pcf8591, at24c02);
        let (scl_high, sda_high) = self.line_levels(
            master_scl_high,
            master_sda_high,
            slave_scl_low,
            slave_sda_low,
        );

        pcf8591.sample_i2c(time_ns, scl_high, sda_high, analog);
        at24c02.sample_i2c(time_ns, scl_high, sda_high);

        let (slave_scl_low, slave_sda_low) = self.slave_drives_low(pcf8591, at24c02);
        let (_, settled_sda_high) = self.line_levels(
            master_scl_high,
            master_sda_high,
            slave_scl_low,
            slave_sda_low,
        );
        pcf8591.settle_i2c_lines(settled_sda_high);
        at24c02.settle_i2c_lines(settled_sda_high);
    }

    pub(crate) fn slave_drives_low(&self, pcf8591: &Pcf8591, at24c02: &At24c02) -> (bool, bool) {
        (
            pcf8591.scl_drive_low() || at24c02.scl_drive_low(),
            pcf8591.sda_drive_low() || at24c02.sda_drive_low(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::I2cBus;
    use crate::peripherals::{AnalogInputs, At24c02, Pcf8591};

    struct I2cHarness {
        bus: I2cBus,
        analog: AnalogInputs,
        pcf8591: Pcf8591,
        at24c02: At24c02,
        scl: bool,
        sda: bool,
        time_ns: u64,
    }

    impl Default for I2cHarness {
        fn default() -> Self {
            let mut harness = Self {
                bus: I2cBus,
                analog: AnalogInputs::default(),
                pcf8591: Pcf8591::default(),
                at24c02: At24c02::default(),
                scl: true,
                sda: true,
                time_ns: 0,
            };
            harness.tick();
            harness
        }
    }

    impl I2cHarness {
        const STEP_NS: u64 = 10_000;

        fn tick(&mut self) {
            self.bus.sample(
                self.time_ns,
                self.scl,
                self.sda,
                &self.analog,
                &mut self.pcf8591,
                &mut self.at24c02,
            );
        }

        fn wait_ns(&mut self, ns: u64) {
            self.time_ns = self.time_ns.saturating_add(ns);
            self.tick();
        }

        fn wait_us(&mut self, us: u64) {
            self.wait_ns(us.saturating_mul(1_000));
        }

        fn set_scl(&mut self, level: bool) {
            self.scl = level;
            self.tick();
        }

        fn set_sda(&mut self, level: bool) {
            self.sda = level;
            self.tick();
        }

        fn line_sda_high(&self) -> bool {
            let (slave_scl_low, slave_sda_low) =
                self.bus.slave_drives_low(&self.pcf8591, &self.at24c02);
            let (_, sda_high) =
                self.bus
                    .line_levels(self.scl, self.sda, slave_scl_low, slave_sda_low);
            sda_high
        }

        fn start(&mut self) {
            self.set_sda(true);
            self.wait_ns(Self::STEP_NS);
            self.set_scl(true);
            self.wait_ns(Self::STEP_NS);
            self.set_sda(false);
            self.wait_ns(Self::STEP_NS);
            self.set_scl(false);
            self.wait_ns(Self::STEP_NS);
        }

        fn stop(&mut self) {
            self.set_sda(false);
            self.wait_ns(Self::STEP_NS);
            self.set_scl(true);
            self.wait_ns(Self::STEP_NS);
            self.set_sda(true);
            self.wait_ns(Self::STEP_NS);
        }

        fn send_byte(&mut self, byte: u8) {
            let mut value = byte;
            for _ in 0..8 {
                self.set_scl(false);
                self.wait_ns(Self::STEP_NS);
                self.set_sda(value & 0x80 != 0);
                self.wait_ns(Self::STEP_NS);
                self.set_scl(true);
                self.wait_ns(Self::STEP_NS);
                value <<= 1;
            }
            self.set_scl(false);
            self.wait_ns(Self::STEP_NS);
        }

        fn wait_ack(&mut self) -> bool {
            self.set_sda(true);
            self.wait_ns(Self::STEP_NS);
            self.set_scl(true);
            self.wait_ns(Self::STEP_NS);
            let ack = self.line_sda_high();
            self.set_scl(false);
            self.wait_ns(Self::STEP_NS);
            ack
        }

        fn receive_byte(&mut self) -> u8 {
            let mut value = 0_u8;
            self.set_sda(true);
            self.wait_ns(Self::STEP_NS);
            for _ in 0..8 {
                self.set_scl(true);
                self.wait_ns(Self::STEP_NS);
                value = (value << 1) | u8::from(self.line_sda_high());
                self.set_scl(false);
                self.wait_ns(Self::STEP_NS);
            }
            value
        }

        fn send_ack(&mut self, nack: bool) {
            self.send_ack_with_release_delay(nack, Self::STEP_NS);
        }

        fn send_ack_with_release_delay(&mut self, nack: bool, release_delay_ns: u64) {
            self.set_scl(false);
            self.wait_ns(Self::STEP_NS);
            self.set_sda(nack);
            self.wait_ns(Self::STEP_NS);
            self.set_scl(true);
            self.wait_ns(Self::STEP_NS);
            self.set_scl(false);
            self.wait_ns(release_delay_ns);
            self.set_sda(true);
            self.wait_ns(Self::STEP_NS);
        }

        fn glitch_scl_low_us(&mut self, us: u64) {
            self.set_scl(false);
            self.wait_us(us);
            self.set_scl(true);
        }

        fn eeprom_write_byte(&mut self, addr: u8, value: u8) {
            self.start();
            self.send_byte(0xA0);
            assert!(!self.wait_ack());
            self.send_byte(addr);
            assert!(!self.wait_ack());
            self.send_byte(value);
            assert!(!self.wait_ack());
            self.stop();
        }

        fn eeprom_random_read_start(&mut self, addr: u8) {
            self.start();
            self.send_byte(0xA0);
            assert!(!self.wait_ack(), "EEPROM 应响应写地址");
            self.send_byte(addr);
            assert!(!self.wait_ack(), "EEPROM 应响应字地址");
            self.start();
            self.send_byte(0xA1);
            assert!(!self.wait_ack(), "重复 START 后 EEPROM 应响应读地址");
        }

        fn eeprom_read_byte(&mut self, addr: u8) -> u8 {
            self.eeprom_random_read_start(addr);
            let value = self.receive_byte();
            self.send_ack(true);
            self.stop();
            value
        }

        fn eeprom_read_byte_with_ack_glitch(&mut self, addr: u8, glitch_us: u64) -> u8 {
            self.start();
            self.send_byte(0xA0);
            assert!(!self.wait_ack());
            self.send_byte(addr);
            assert!(!self.wait_ack());
            self.start();
            self.send_byte(0xA1);
            self.set_sda(true);
            self.wait_ns(Self::STEP_NS);
            self.set_scl(true);
            self.wait_ns(Self::STEP_NS);
            assert!(!self.line_sda_high());
            self.glitch_scl_low_us(glitch_us);
            self.wait_ns(Self::STEP_NS);
            assert!(!self.line_sda_high());
            self.set_scl(false);
            self.wait_ns(Self::STEP_NS);
            let value = self.receive_byte();
            self.send_ack(true);
            self.stop();
            value
        }
    }

    #[test]
    fn eeprom_random_read_returns_written_values() {
        let mut harness = I2cHarness::default();
        harness.eeprom_write_byte(0x00, 0x01);
        harness.wait_us(5_100);
        harness.eeprom_write_byte(0x01, 0x02);
        harness.wait_us(5_100);
        harness.eeprom_write_byte(0x02, 0x03);
        harness.wait_us(5_100);
        harness.eeprom_write_byte(0x20, 0x09);
        harness.wait_us(5_100);

        assert_eq!(harness.eeprom_read_byte(0x00), 0x01);
        assert_eq!(harness.eeprom_read_byte(0x01), 0x02);
        assert_eq!(harness.eeprom_read_byte(0x02), 0x03);
        assert_eq!(harness.eeprom_read_byte(0x20), 0x09);
        assert_eq!(harness.eeprom_read_byte(0x00), 0x01);
    }

    #[test]
    fn eeprom_random_read_ignores_short_scl_glitch_during_ack() {
        let mut harness = I2cHarness::default();
        harness.eeprom_write_byte(0x00, 0x01);
        harness.wait_us(5_100);
        harness.eeprom_write_byte(0x01, 0x02);
        harness.wait_us(5_100);

        assert_eq!(harness.eeprom_read_byte_with_ack_glitch(0x00, 1), 0x01);
        assert_eq!(harness.eeprom_read_byte_with_ack_glitch(0x01, 1), 0x02);
    }

    fn assert_eeprom_sequential_read(release_delay_ns: u64) {
        let mut harness = I2cHarness::default();
        harness
            .at24c02
            .set_bytes(0, &[0x00, 0x55])
            .expect("预置 EEPROM");
        harness.eeprom_random_read_start(0);
        assert_eq!(harness.receive_byte(), 0, "随机读起始应返回首字节");
        harness.send_ack_with_release_delay(false, release_delay_ns);
        assert_eq!(
            harness.receive_byte(),
            0x55,
            "ACK 下降后 {release_delay_ns}ns 释放 SDA 应继续顺序读"
        );
        harness.send_ack(true);
        harness.stop();
        assert_eq!(
            harness.eeprom_read_byte(1),
            0x55,
            "NACK/STOP 后应可重新寻址"
        );
    }

    #[test]
    fn eeprom_sequential_read_releases_ack_after_84ns() {
        assert_eeprom_sequential_read(84);
    }

    #[test]
    fn eeprom_sequential_read_releases_ack_after_1us() {
        assert_eeprom_sequential_read(1_000);
    }

    #[test]
    fn eeprom_sequential_read_allows_zero_data_hold_and_filter_boundary() {
        for release_delay_ns in [0, 1_499, 1_500, 1_501, 5_000] {
            assert_eeprom_sequential_read(release_delay_ns);
        }
    }

    #[test]
    fn eeprom_real_stop_commits_write_and_ends_transaction() {
        let mut harness = I2cHarness::default();
        harness.start();
        for byte in [0xA0, 0x10, 0x55] {
            harness.send_byte(byte);
            assert!(!harness.wait_ack(), "EEPROM 应确认写入字节 {byte:#04x}");
        }
        assert_eq!(harness.at24c02.byte(0x10), 0, "STOP 前不得提交缓存");
        harness.stop();
        assert_eq!(
            harness.at24c02.byte(0x10),
            0x55,
            "真实高 SCL STOP 应提交写入"
        );

        harness.send_byte(0xA0);
        assert!(harness.wait_ack(), "STOP 后缺少 START 的字节不得开启事务");
        harness.start();
        harness.send_byte(0xA0);
        assert!(harness.wait_ack(), "真实 STOP 应启动 EEPROM 写忙周期");
        harness.stop();
        harness.wait_us(5_100);
        assert_eq!(
            harness.eeprom_read_byte(0x10),
            0x55,
            "写忙结束后应读回提交值"
        );
    }

    #[test]
    fn eeprom_write_ignores_short_sda_glitches_while_scl_is_high() {
        // AT24C02 手册第 5 页规定 5V 输入噪声抑制为 50ns，覆盖两种极性。
        for glitch_ns in [1, 49, 50] {
            for value in [0x55_u8, 0xA5] {
                let mut harness = I2cHarness::default();
                harness.start();
                for byte in [0xA0, 0x10, 0x11] {
                    harness.send_byte(byte);
                    assert!(!harness.wait_ack(), "毛刺前应正常接收写事务");
                }
                for bit in (0..8).rev() {
                    let level = value & (1 << bit) != 0;
                    harness.set_scl(false);
                    harness.wait_ns(I2cHarness::STEP_NS);
                    harness.set_sda(level);
                    harness.wait_ns(I2cHarness::STEP_NS);
                    harness.set_scl(true);
                    harness.wait_ns(I2cHarness::STEP_NS);
                    if bit == 7 {
                        harness.set_sda(!level);
                        harness.wait_ns(glitch_ns);
                        harness.set_sda(level);
                        harness.wait_ns(I2cHarness::STEP_NS);
                        assert_eq!(
                            harness.at24c02.byte(0x10),
                            0,
                            "{glitch_ns}ns SDA 毛刺不得提交写缓存"
                        );
                    }
                }
                harness.set_scl(false);
                harness.wait_ns(I2cHarness::STEP_NS);
                assert!(!harness.wait_ack(), "SDA 毛刺不得重置当前字节");
                harness.stop();
                assert_eq!(harness.at24c02.byte(0x10), 0x11, "首字节应保留");
                assert_eq!(
                    harness.at24c02.byte(0x11),
                    value,
                    "毛刺后应完整接收下一字节"
                );
            }
        }
    }

    #[test]
    fn eeprom_write_ignores_short_scl_high_glitch() {
        let mut harness = I2cHarness::default();
        harness.start();
        harness.send_byte(0xA0);
        assert!(!harness.wait_ack(), "EEPROM 应响应写地址");
        harness.set_sda(false);
        harness.wait_ns(I2cHarness::STEP_NS);
        harness.set_scl(true);
        harness.wait_ns(1_000);
        harness.set_scl(false);
        harness.wait_ns(I2cHarness::STEP_NS);
        for byte in [0x10, 0xA5] {
            harness.send_byte(byte);
            assert!(!harness.wait_ack(), "短 SCL 高脉冲不得推进数据位");
        }
        harness.stop();
        assert_eq!(
            harness.at24c02.byte(0x10),
            0xA5,
            "过滤 SCL 毛刺后应正常提交"
        );
    }

    #[test]
    fn pcf8591_write_ignores_sda_spikes_up_to_100ns() {
        // PCF8591 手册 p19 的总线可容忍尖峰 tSP 最大为 100ns。
        for glitch_ns in [51, 99, 100] {
            for value in [0x55_u8, 0xA5] {
                let mut harness = I2cHarness::default();
                harness.start();
                for byte in [0x90, 0x40] {
                    harness.send_byte(byte);
                    assert!(!harness.wait_ack(), "PCF8591 应接收地址和控制字");
                }
                for bit in (0..8).rev() {
                    let level = value & (1 << bit) != 0;
                    harness.set_scl(false);
                    harness.wait_ns(I2cHarness::STEP_NS);
                    harness.set_sda(level);
                    harness.wait_ns(I2cHarness::STEP_NS);
                    harness.set_scl(true);
                    harness.wait_ns(I2cHarness::STEP_NS);
                    if bit == 7 {
                        harness.set_sda(!level);
                        harness.wait_ns(glitch_ns);
                        harness.set_sda(level);
                        harness.wait_ns(I2cHarness::STEP_NS);
                    }
                }
                harness.set_scl(false);
                harness.wait_ns(I2cHarness::STEP_NS);
                assert!(
                    !harness.wait_ack(),
                    "{glitch_ns}ns SDA 毛刺不得打断 DAC 写入"
                );
                harness.stop();
                assert_eq!(
                    harness.pcf8591.dac_value(),
                    value,
                    "毛刺后应完整接收 DAC 码"
                );
            }
        }
    }

    #[test]
    fn pcf8591_sequential_read_releases_ack_early_on_shared_bus() {
        for release_delay_ns in [84, 1_000] {
            let mut harness = I2cHarness::default();
            harness.analog.set_voltage("AIN1", 0.0).expect("设置 AIN1");
            harness.at24c02.set_byte(0x20, 0xA5);
            harness.start();
            for byte in [0x90, 0x41] {
                harness.send_byte(byte);
                assert!(!harness.wait_ack(), "PCF8591 应确认地址和控制字");
            }
            harness.start();
            harness.send_byte(0x91);
            assert!(!harness.wait_ack(), "重复 START 后 PCF8591 应响应读地址");
            assert_eq!(harness.receive_byte(), 0x80, "PCF8591 首读应为上电旧值");
            harness.send_ack_with_release_delay(false, release_delay_ns);
            assert_eq!(
                harness.receive_byte(),
                0,
                "PCF8591 提早释放 ACK 后应继续发送转换值"
            );
            harness.send_ack(true);
            harness.stop();
            assert_eq!(
                harness.eeprom_read_byte(0x20),
                0xA5,
                "共享总线的另一从机不得被 settle 边沿破坏"
            );
        }
    }
}
