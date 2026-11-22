use super::WebSim;
use serde_json::json;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

// STC15_DS.pdf pp. 285–286, 306 (registers/PSW), p. 324 §4.9.1 (pins versus latches).
// Names follow the existing core chip/registers.rs and vendor/i8051/src/sfr.rs mappings.
const NAMED_SFR: &[(u8, &str)] = &[
    (0x80, "P0"),
    (0x81, "SP"),
    (0x82, "DPL"),
    (0x83, "DPH"),
    (0x87, "PCON"),
    (0x88, "TCON"),
    (0x89, "TMOD"),
    (0x8A, "TL0"),
    (0x8B, "TL1"),
    (0x8C, "TH0"),
    (0x8D, "TH1"),
    (0x8E, "AUXR"),
    (0x8F, "INT_CLKO"),
    (0x90, "P1"),
    (0x91, "P1M1"),
    (0x92, "P1M0"),
    (0x93, "P0M1"),
    (0x94, "P0M0"),
    (0x95, "P2M1"),
    (0x96, "P2M0"),
    (0x97, "CLK_DIV"),
    (0x98, "SCON"),
    (0x99, "SBUF"),
    (0x9A, "S2CON"),
    (0x9B, "S2BUF"),
    (0xA0, "P2"),
    (0xA2, "AUXR1/P_SW1"),
    (0xA8, "IE"),
    (0xAF, "IE2"),
    (0xB0, "P3"),
    (0xB1, "P3M1"),
    (0xB2, "P3M0"),
    (0xB3, "P4M1"),
    (0xB4, "P4M0"),
    (0xB5, "IP2"),
    (0xB8, "IP"),
    (0xBA, "P_SW2"),
    (0xC0, "P4"),
    (0xD0, "PSW"),
    (0xD6, "T2H"),
    (0xD7, "T2L"),
    (0xD8, "CCON"),
    (0xD9, "CMOD"),
    (0xDA, "CCAPM0"),
    (0xDB, "CCAPM1"),
    (0xE0, "ACC"),
    (0xE9, "CL"),
    (0xEA, "CCAP0L"),
    (0xEB, "CCAP1L"),
    (0xF0, "B"),
    (0xF9, "CH"),
    (0xFA, "CCAP0H"),
    (0xFB, "CCAP1H"),
];

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl WebSim {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = debugState))]
    pub fn debug_state(&self) -> String {
        let read = |addr| self.sim.peek_sfr(addr).expect("valid SFR address");
        let psw = read(0xD0);
        let bank = (psw >> 3) & 3;
        let iram: Vec<u8> = (0..=255).map(|addr| self.sim.peek_iram(addr)).collect();
        let sfr: Vec<_> = NAMED_SFR.iter().map(|&(address, name)| {
            json!({ "address": address, "name": name, "value": read(address) })
        }).collect();
        let ports: Vec<_> = [0x80, 0x90, 0xA0, 0xB0, 0xC0].iter().enumerate().map(|(index, &addr)| {
            json!({ "name": format!("P{index}"), "latch": self.sim.peek_sfr_latch(addr).expect("valid port"), "pins": read(addr) })
        }).collect();
        json!({
            "pc": self.sim.snapshot().pc,
            "a": read(0xE0), "b": read(0xF0), "psw": psw, "sp": read(0x81),
            "dptr": u16::from(read(0x83)) << 8 | u16::from(read(0x82)),
            "bank": bank, "r": &iram[usize::from(bank) * 8..usize::from(bank) * 8 + 8],
            "flags": { "cy": psw & 0x80 != 0, "ac": psw & 0x40 != 0, "f0": psw & 0x20 != 0,
                "rs1": psw & 0x10 != 0, "rs0": psw & 0x08 != 0, "ov": psw & 0x04 != 0,
                "f1": psw & 0x02 != 0, "p": psw & 0x01 != 0 },
            "iram": iram, "sfr": sfr, "ports": ports,
        })
        .to_string()
    }
}
