use std::collections::BTreeMap;

use anyhow::Result;

use crate::ids::VoltageChannel;

#[derive(Debug, Default, Clone)]
pub(crate) struct AnalogInputs {
    voltages: BTreeMap<String, f32>,
    pub(crate) rb2_position: f32,
    pub(crate) light_level: f32,
}

impl AnalogInputs {
    pub(crate) fn set_voltage(&mut self, name: &str, value: f32) -> Result<()> {
        self.set_voltage_channel(VoltageChannel::parse(name)?, value);
        Ok(())
    }

    pub(crate) fn set_voltage_channel(&mut self, channel: VoltageChannel, value: f32) {
        self.voltages
            .insert(channel.as_str().to_string(), value.clamp(0.0, 5.0));
    }

    pub(crate) fn channel_voltage(&self, channel: u8) -> f32 {
        match channel {
            1 => *self.voltages.get("RD1").unwrap_or(&0.0),
            3 => *self.voltages.get("RB2").unwrap_or(&0.0),
            _ => 0.0,
        }
    }

    pub(crate) fn channel_value(&self, channel: u8) -> u8 {
        ((self.channel_voltage(channel) / 5.0) * 256.0)
            .round()
            .clamp(0.0, 255.0) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::AnalogInputs;

    #[test]
    fn adc_reference_scale_matches_datasheet_boundaries() {
        let mut analog = AnalogInputs::default();
        let vref = 5.0;

        for (name, channel) in [("AIN1", 1), ("AIN3", 3)] {
            for (voltage, expected) in [
                (0.0, 0),
                (vref / 256.0, 1),
                (vref / 2.0, 128),
                (vref * 255.0 / 256.0, 255),
                (vref, 255),
            ] {
                analog.set_voltage(name, voltage).expect("设置输入电压");
                assert_eq!(
                    analog.channel_value(channel),
                    expected,
                    "{name} 输入 {voltage}V 的 ADC 代码"
                );
            }
        }
    }

    #[test]
    fn adc_reference_scale_rounds_at_half_lsb() {
        let mut analog = AnalogInputs::default();
        let lsb = 5.0 / 256.0;

        for (fraction, expected) in [(0.49, 0), (0.50, 1), (0.51, 1)] {
            analog
                .set_voltage("AIN1", fraction * lsb)
                .expect("设置输入电压");
            assert_eq!(
                analog.channel_value(1),
                expected,
                "输入 {fraction} LSB 时应在半 LSB 处四舍五入"
            );
        }
    }
}
