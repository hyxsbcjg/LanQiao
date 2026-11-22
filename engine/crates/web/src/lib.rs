mod board_controls;
mod debug;
mod error;

pub use error::WebError;
use error::{error, validate_range};
use serde_json::json;
use stcjudge::{KeyMode, ResetMode, SignalId, Simulator};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct WebSim {
    sim: Simulator,
    loaded: bool,
    diagnostic: Option<String>,
}

impl Default for WebSim {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl WebSim {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new() -> Self {
        let mut sim = Simulator::nop(false);
        sim.set_rb3(0.5).expect("valid initial RB3 position");
        sim.set_rb2(0.0).expect("valid initial RB2 position");
        sim.set_light(0.0).expect("valid initial light level");
        Self {
            sim,
            loaded: false,
            diagnostic: None,
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = loadHex))]
    pub fn load_hex(&mut self, text: &str) -> Result<(), WebError> {
        let mut next = Simulator::from_hex_text(text, false).map_err(error)?;
        next.copy_board_inputs_from(&self.sim);
        next.set_uart_terminal_baud(self.sim.uart_terminal_baud())
            .map_err(error)?;
        next.reset_with_mode(ResetMode::Power).map_err(error)?;
        self.sim = next;
        self.loaded = true;
        self.diagnostic = None;
        Ok(())
    }

    pub fn reset(&mut self, mode: &str) -> Result<(), WebError> {
        let mode = match mode {
            "mcu" => ResetMode::Cpu,
            "power" => ResetMode::Power,
            _ => return Err(error("复位模式必须为 mcu 或 power")),
        };
        self.sim.reset_with_mode(mode).map_err(error)?;
        self.diagnostic = None;
        Ok(())
    }

    pub fn set_irc_hz(&mut self, hz: u32) -> Result<(), WebError> {
        self.sim.set_irc_hz(u64::from(hz)).map_err(error)?;
        self.diagnostic = None;
        Ok(())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = runUs))]
    pub fn run_us(&mut self, us: u32) {
        if self.loaded && self.diagnostic.is_none() {
            self.diagnostic = self
                .sim
                .run_us(u64::from(us))
                .err()
                .map(|err| format!("{err:#}"));
        }
    }

    pub fn step(&mut self) {
        if self.loaded && self.diagnostic.is_none() {
            self.diagnostic = self
                .sim
                .step_instruction()
                .err()
                .map(|err| format!("{err:#}"));
        }
    }

    pub fn snapshot(&self) -> String {
        let mut value = self.snapshot_value();
        let (events, dropped) = self.sim.take_relay_events();
        if !events.is_empty() {
            value["relayEvents"] = events
                .iter()
                .map(|event| json!({"tNs": event.t_ns, "on": event.on}))
                .collect();
        }
        if dropped != 0 {
            value["relayEventsDropped"] = json!(dropped);
        }
        value.to_string()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = peekSnapshot))]
    pub fn peek_snapshot(&self) -> String {
        self.snapshot_value().to_string()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = takeBrightness))]
    pub fn take_brightness(&mut self) -> String {
        let value = self.sim.take_brightness();
        json!({ "windowNs": value.window_ns, "seg": value.seg, "led": value.led }).to_string()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setKey))]
    pub fn set_key(&mut self, key: &str, pressed: bool) -> Result<(), WebError> {
        self.sim.set_key(key, pressed).map_err(error)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setKeyMode))]
    pub fn set_key_mode(&mut self, mode: &str) -> Result<(), WebError> {
        let mode = match mode {
            "KBD" => KeyMode::Keyboard,
            "BTN" => KeyMode::Button,
            _ => return Err(error("按键模式必须为 KBD 或 BTN")),
        };
        self.sim.key_mode(mode);
        Ok(())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setTemperatureC))]
    pub fn set_temperature(&mut self, value: f32) -> Result<(), WebError> {
        validate_range(value, -55.0, 125.0, "温度（℃）")?;
        self.sim.set_temperature_c(value);
        Ok(())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setVoltage))]
    pub fn set_voltage(&mut self, channel: &str, value: f32) -> Result<(), WebError> {
        if !matches!(channel, "AIN1" | "AIN3") {
            return Err(error("电压通道必须为 AIN1 或 AIN3"));
        }
        validate_range(value, 0.0, 5.0, "电压（V）")?;
        self.sim.set_voltage(channel, value).map_err(error)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setFrequencyHz))]
    pub fn set_frequency(&mut self, value: f32) -> Result<(), WebError> {
        validate_range(value, 0.0, f32::MAX, "频率（Hz）")?;
        self.sim.set_frequency_hz(value);
        Ok(())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setDistanceCm))]
    pub fn set_distance(&mut self, value: f32) -> Result<(), WebError> {
        validate_range(value, 0.0, f32::MAX, "距离（cm）")?;
        self.sim.set_distance_cm(value);
        Ok(())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setNe555Jumper))]
    pub fn set_jumper(&mut self, installed: bool) -> Result<(), WebError> {
        if installed {
            self.sim.jumper_on(SignalId::NetSig, SignalId::SigOut)
        } else {
            self.sim.jumper_off(SignalId::NetSig, SignalId::SigOut)
        }
        .map_err(error)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setRtc))]
    pub fn set_rtc(&mut self, hour: u32, minute: u32, second: u32) -> Result<(), WebError> {
        if hour > 23 || minute > 59 || second > 59 {
            return Err(error("RTC 时间必须满足时 0..=23、分秒 0..=59"));
        }
        self.sim
            .set_rtc(hour as u8, minute as u8, second as u8)
            .map_err(error)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = uartWrite))]
    pub fn uart_write(&mut self, text: &str) -> Result<(), WebError> {
        let bytes: Result<Vec<u8>, _> =
            text.chars().map(|ch| u8::try_from(u32::from(ch))).collect();
        let bytes = bytes.map_err(|_| error("UART requires byte characters U+0000..U+00FF"))?;
        self.sim.uart_write(&bytes).map_err(error)
    }

    pub fn set_uart_terminal_baud(&mut self, baud: u32) -> Result<(), WebError> {
        self.sim.set_uart_terminal_baud(baud).map_err(error)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = uartTake))]
    pub fn uart_take(&mut self) -> Result<String, WebError> {
        let symbols = self.sim.uart_peek_raw();
        if symbols.iter().any(|symbol| *symbol > 255) {
            return Err(error("UART byte API cannot represent 9-bit symbols"));
        }
        Ok(self
            .sim
            .uart_take_raw()
            .into_iter()
            .map(|symbol| char::from(symbol as u8))
            .collect())
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = exportState))]
    pub fn export_persistent_state(&self) -> String {
        self.sim.export_persistent_state()
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = importState))]
    pub fn import_persistent_state(&mut self, text: &str) -> Result<(), WebError> {
        self.sim.load_persistent_state(text).map_err(error)
    }
}

impl WebSim {
    fn snapshot_value(&self) -> serde_json::Value {
        let board = self.sim.snapshot();
        let digits: [Option<u8>; 8] = std::array::from_fn(|index| {
            if self.loaded {
                self.sim.seg_raw(index + 1).ok()
            } else {
                None
            }
        });
        json!({
            "ircHz": self.sim.irc_hz(),
            "timeNs": board.sim_time_ns,
            "pc": board.pc,
            "digits": digits,
            "leds": if self.loaded { board.led_states } else { [false; 8] },
            "relay": self.loaded && board.relay_on,
            "motor": self.loaded && board.motor_on,
            "buzzer": self.loaded && board.buzzer_on,
            "diagnostic": self.diagnostic,
            "uart": { "fwBaud": self.sim.uart1_firmware_baud(), "termBaud": self.sim.uart_terminal_baud() },
        })
    }
}

#[cfg(test)]
mod tests;
