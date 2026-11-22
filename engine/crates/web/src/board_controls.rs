use super::{WebError, WebSim, error, validate_range};
use serde_json::json;
use stcjudge::KeyMode;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl WebSim {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setJumper))]
    pub fn set_board_jumper(&mut self, id: &str, position: &str) -> Result<(), WebError> {
        match (id, position) {
            ("J5", "KBD" | "BTN") => self.set_key_mode(position),
            ("J13", "IO" | "MM") => self.sim.set_latch_mode(position).map_err(error),
            ("J6", "ON" | "OFF") => {
                self.sim.set_load_power(position == "ON");
                Ok(())
            }
            ("J2", "US" | "IR") => self.sim.set_ultrasonic_route(position).map_err(error),
            ("J3", "NE555_ON" | "NE555_OFF") => self.set_jumper(position == "NE555_ON"),
            _ => Err(error("Invalid jumper ID or position")),
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setDial))]
    pub fn set_dial(&mut self, id: &str, position: f32) -> Result<(), WebError> {
        validate_range(position, 0.0, 1.0, "Dial position")?;
        match id {
            "RB2" => self.sim.set_rb2(position).map_err(error),
            "RB3" => self.sim.set_rb3(position).map_err(error),
            _ => Err(error("Dial ID must be RB2 or RB3")),
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setLight))]
    pub fn set_light(&mut self, level: f32) -> Result<(), WebError> {
        validate_range(level, 0.0, 1.0, "Light level")?;
        self.sim.set_light(level).map_err(error)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = boardState))]
    pub fn board_state(&self) -> String {
        let board = self.sim.snapshot();
        json!({
            "jumpers": {
                "J5": if board.key_mode == KeyMode::Keyboard { "KBD" } else { "BTN" },
                "J13": self.sim.latch_mode(),
                "J6": if self.sim.load_power() { "ON" } else { "OFF" },
                "J2": self.sim.ultrasonic_route(),
                "J3": if board.jumper_net_sig_to_sig_out { "NE555_ON" } else { "NE555_OFF" },
            },
            "dials": { "RB2": self.sim.rb2_position(), "RB3": self.sim.rb3_position() },
            "light": self.sim.light_level(),
            "derived": {
                "rb3ResistanceOhm": self.sim.rb3_resistance_ohm(),
                "ldrResistanceOhm": self.sim.ldr_resistance_ohm(),
                "ain1V": self.sim.light_voltage_v(),
                "ain3V": self.sim.rb2_voltage_v(),
                "ne555Hz": self.sim.rb3_frequency_hz(),
            },
            "ain1V": board.analog_rd1_v,
            "ain3V": board.analog_rb2_v,
            "ne555Hz": board.ne555_frequency_hz,
        })
        .to_string()
    }
}
