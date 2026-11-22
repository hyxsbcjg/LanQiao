use super::WebSim;
use serde_json::{Value, json};

#[test]
fn irc_download_validates_preserves_eeprom_and_clock_across_loads() {
    let mut sim = WebSim::new();
    assert_eq!(snapshot(&sim)["ircHz"], 12_000_000);
    sim.set_irc_hz(35_000_000).unwrap();
    sim.load_hex(&hex(&[0x75, 0x40, 0x5a, 0x80, 0xfe])).unwrap();
    sim.sim.set_eeprom_byte(7, 0x69);
    sim.step();
    assert_eq!(sim.sim.peek_iram(0x40), 0x5a);
    for hz in [0, 4_999_999, 35_000_001, u32::MAX] {
        assert!(sim.set_irc_hz(hz).is_err());
        assert_eq!(sim.sim.peek_iram(0x40), 0x5a);
    }
    sim.set_irc_hz(35_000_000).unwrap();
    assert_eq!(snapshot(&sim)["ircHz"], 35_000_000);
    assert_eq!(snapshot(&sim)["pc"], 0);
    assert_eq!(sim.sim.peek_iram(0x40), 0);
    assert_eq!(sim.sim.eeprom_byte(7), 0x69);
    sim.step();
    assert_eq!(sim.sim.peek_iram(0x40), 0x5a);
    assert_eq!(snapshot(&sim)["diagnostic"], Value::Null);
}

fn hex(code: &[u8]) -> String {
    let mut bytes = vec![code.len() as u8, 0, 0, 0];
    bytes.extend_from_slice(code);
    let checksum = 0_u8.wrapping_sub(bytes.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte)));
    let data: String = bytes.iter().map(|byte| format!("{byte:02X}")).collect();
    format!(":{data}{checksum:02X}\n:00000001FF\n")
}

fn snapshot(sim: &WebSim) -> Value {
    serde_json::from_str(&sim.snapshot()).unwrap()
}

// SCH_V31 p3 / V2 U9 Q4 -> ULN2003 -> K1; 74HC573 p6 Table 4.
// Close LE before changing shared P0. Initial power-on Q4 is high in this model.
fn relay_sim() -> WebSim {
    let mut sim = WebSim::new();
    sim.load_hex(&hex(&[
        0x75, 0xA0, 0, // MOV P2,#0
        0x75, 0x80, 0, // MOV P0,#0
        0x75, 0xA0, 0xA0, // MOV P2,#0A0H: Q4 goes low
        0x75, 0x80, 0x10, // MOV P0,#10H: Q4 goes high while LE stays high
        0x75, 0x80, 0x10, // Same value: no transition
        0x75, 0x80, 0, // Q4 goes low
        0x80, 0xFE,
    ]))
    .unwrap();
    for _ in 0..3 {
        sim.step();
    }
    sim.snapshot(); // Drain the initialization falling edge.
    sim
}

#[test]
fn relay_event_single_toggle_and_same_value_rewrite() {
    let mut sim = relay_sim();
    sim.step();
    let on = snapshot(&sim);
    assert_eq!(on["relayEvents"], json!([{ "tNs": on["timeNs"], "on": true }]));
    assert!(snapshot(&sim).get("relayEvents").is_none());
    sim.step();
    assert!(snapshot(&sim).get("relayEvents").is_none());
    sim.step();
    let off = snapshot(&sim);
    assert_eq!(off["relayEvents"], json!([{ "tNs": off["timeNs"], "on": false }]));
}

#[test]
fn relay_events_peeks_do_not_drain_pending_step() {
    let mut sim = relay_sim();
    sim.step();
    for _ in 0..3 {
        let peek: Value = serde_json::from_str(&sim.peek_snapshot()).unwrap();
        assert_eq!(peek["relay"], true);
        assert!(peek.get("relayEvents").is_none());
    }
    assert_eq!(snapshot(&sim)["relayEvents"][0]["on"], true);
    assert!(snapshot(&sim).get("relayEvents").is_none());
}

#[test]
fn relay_events_keep_many_transitions_in_one_snapshot_and_ignore_vb() {
    let mut sim = relay_sim();
    sim.set_board_jumper("J6", "OFF").unwrap();
    for _ in 0..3 {
        sim.step();
    }
    let value = snapshot(&sim);
    let events = value["relayEvents"].as_array().expect("relay transitions");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["on"], true);
    assert_eq!(events[1]["on"], false);
    assert!(events[0]["tNs"].as_u64().unwrap() < events[1]["tNs"].as_u64().unwrap());
    assert_eq!(value["relay"], false);
    sim.set_board_jumper("J6", "ON").unwrap();
    assert!(snapshot(&sim).get("relayEvents").is_none());
}

#[test]
fn relay_events_cap_keeps_first_256_and_counts_further_transitions() {
    let mut sim = relay_sim();
    sim.set_board_jumper("J13", "MM").unwrap();
    let mut expected = Vec::new();
    for index in 0..300 {
        sim.step();
        let on = index % 2 == 0;
        sim.sim.poke_xdata(0xA000, if on { 0x10 } else { 0 });
        if index < 256 {
            expected.push(json!({"tNs": sim.sim.sim_time_ns(), "on": on}));
        }
    }
    let value = snapshot(&sim);
    assert_eq!(value["relayEvents"], json!(expected));
    assert_eq!(value["relayEventsDropped"], 44);
    let empty = snapshot(&sim);
    assert!(empty.get("relayEvents").is_none());
    assert!(empty.get("relayEventsDropped").is_none());
}

#[test]
fn relay_events_survive_resets_and_load_clears_them() {
    for mode in ["mcu", "power"] {
        let mut sim = relay_sim();
        sim.step(); // Pending rising edge, not yet published.
        sim.reset(mode).unwrap();
        let value = snapshot(&sim);
        assert_eq!(value["relayEvents"].as_array().unwrap().len(), 1);
        assert_eq!(value["relayEvents"][0]["on"], true);
        // CPU reset retains Y5; power reset restores the existing high power-on Q4.
        // Firmware's subsequent clear must emit a falling edge in either case.
        for _ in 0..3 {
            sim.step();
        }
        assert_eq!(snapshot(&sim)["relayEvents"][0]["on"], false);
        sim.step();
        sim.load_hex(&hex(&[0x80, 0xFE])).unwrap();
        assert!(snapshot(&sim).get("relayEvents").is_none());
    }
}

#[test]
fn debug_reads_banked_registers_upper_iram_active_dptr_and_port_pins_without_side_effects() {
    let mut sim = WebSim::new();
    sim.load_hex(&hex(&[0x80, 0xFE])).unwrap();
    sim.sim.poke_sfr(0xE0, 0xA5).unwrap();
    sim.sim.poke_sfr(0xF0, 0x5A).unwrap();
    sim.sim.poke_sfr(0x81, 0xD5).unwrap();
    for addr in 0..=255 {
        sim.sim.poke_iram(addr, addr);
    }
    sim.sim.poke_sfr(0x82, 0x34).unwrap();
    sim.sim.poke_sfr(0x83, 0x12).unwrap();
    sim.sim.poke_sfr(0xA2, 1).unwrap();
    sim.sim.poke_sfr(0x82, 0xCD).unwrap();
    sim.sim.poke_sfr(0x83, 0xAB).unwrap();
    sim.set_key_mode("BTN").unwrap();
    sim.set_key("S4", true).unwrap();
    // All reads are peeks: no instruction, flag, UART, brightness, or persistence changes.
    let before = sim.snapshot();
    let persistent = sim.export_persistent_state();
    for bank in 0..4 {
        sim.sim.poke_sfr(0xD0, 0xE5 | (bank << 3)).unwrap();
        let state: Value = serde_json::from_str(&sim.debug_state()).unwrap();
        assert_eq!(state["a"], 0xA5);
        assert_eq!(state["b"], 0x5A);
        assert_eq!(state["sp"], 0xD5);
        assert_eq!(state["psw"], 0xE5 | (bank << 3));
        assert_eq!(state["dptr"], 0xABCD);
        assert_eq!(state["bank"], bank);
        assert_eq!(
            state["r"],
            json!((bank * 8..bank * 8 + 8).collect::<Vec<_>>())
        );
        assert_eq!(state["iram"], json!((0..=255).collect::<Vec<_>>()));
        assert_eq!(state["flags"]["cy"], true);
        assert_eq!(state["flags"]["ac"], true);
        assert_eq!(state["flags"]["ov"], true);
        assert_eq!(state["flags"]["p"], true);
        assert_eq!(state["flags"]["rs1"], bank & 2 != 0);
        assert_eq!(state["flags"]["rs0"], bank & 1 != 0);
        assert_eq!(state["ports"][3]["latch"], 255);
        assert_eq!(state["ports"][3]["pins"].as_u64().unwrap() & 8, 0);
        assert!(
            state["sfr"]
                .as_array()
                .unwrap()
                .iter()
                .any(|sfr| sfr["name"] == "SCON" && sfr["address"] == 0x98)
        );
        assert!(state.get("uart").is_none()); // No public UART config getter exists.
        assert_eq!(sim.debug_state(), sim.debug_state());
    }
    assert_eq!(sim.snapshot(), before);
    assert_eq!(sim.export_persistent_state(), persistent);
    assert_eq!(
        serde_json::from_str::<Value>(&sim.take_brightness()).unwrap()["windowNs"],
        0
    );
    sim.sim.poke_sfr(0xA2, 0).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&sim.debug_state()).unwrap()["dptr"],
        0x1234
    );
}

#[test]
fn brightness_json_is_independent_and_load_and_resets_clear_it() {
    let mut sim = WebSim::new();
    let empty = serde_json::from_str::<Value>(&sim.take_brightness()).unwrap();
    assert_eq!(empty["windowNs"], 0);
    assert_eq!(empty["seg"], json!(([[0.0; 8]; 8])));
    assert_eq!(empty["led"], json!(([0.0; 8])));
    sim.load_hex(&hex(&[0x80, 0xFE])).unwrap();
    sim.run_us(1_000);
    let before = sim.snapshot();
    let value = serde_json::from_str::<Value>(&sim.take_brightness()).unwrap();
    assert_eq!(value["windowNs"], 1_000_000);
    assert_eq!(value["led"], json!(([1.0; 8])));
    assert_eq!(sim.snapshot(), before);
    assert_eq!(
        serde_json::from_str::<Value>(&sim.take_brightness()).unwrap(),
        empty
    );
    for mode in ["mcu", "power"] {
        sim.run_us(1_000);
        sim.reset(mode).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&sim.take_brightness()).unwrap(),
            empty
        );
    }
    sim.run_us(1_000);
    sim.load_hex(&hex(&[0x80, 0xFE])).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&sim.take_brightness()).unwrap(),
        empty
    );
}

#[test]
fn starts_powered_off_and_load_is_transactional() {
    let mut sim = WebSim::new();
    let initial = snapshot(&sim);
    assert_eq!(initial.as_object().unwrap().len(), 10);
    assert_eq!(initial["uart"], json!({"fwBaud": null, "termBaud": 9600}));
    for key in [
        "ircHz",
        "timeNs",
        "pc",
        "digits",
        "leds",
        "relay",
        "motor",
        "buzzer",
        "diagnostic",
        "uart",
    ] {
        assert!(initial.get(key).is_some());
    }
    assert_eq!(initial["digits"], json!(vec![Value::Null; 8]));
    assert_eq!(initial["leds"], json!(vec![false; 8]));
    sim.run_us(100);
    sim.step();
    sim.reset("power").unwrap();
    assert_eq!(snapshot(&sim), initial);
    assert!(sim.load_hex("无效 HEX").is_err());
    assert_eq!(snapshot(&sim), initial);

    sim.load_hex(&hex(&[0x74, 0x01, 0x04, 0x80, 0xFE])).unwrap();
    sim.step();
    assert_eq!(snapshot(&sim)["pc"], 2);
    sim.step();
    assert_eq!(snapshot(&sim)["pc"], 3);
    let running = snapshot(&sim);
    assert!(sim.load_hex("错误 HEX").is_err());
    assert_eq!(snapshot(&sim), running);
}

#[test]
fn step_enters_interrupt_and_executes_one_instruction() {
    let mut sim = WebSim::new();
    sim.load_hex(&hex(&[0; 16])).unwrap();
    sim.sim.poke_sfr(0xA8, 0x82).unwrap();
    sim.sim.poke_sfr(0x88, 0x20).unwrap();
    sim.step();
    assert_eq!(snapshot(&sim)["pc"], 0x0C);
}

#[test]
fn persistent_state_survives_load_and_both_resets() {
    let mut sim = WebSim::new();
    sim.sim.set_eeprom_byte(17, 0xA5);
    sim.set_rtc(12, 34, 56).unwrap();
    let retained = sim.export_persistent_state();
    sim.load_hex(&hex(&[0x80, 0xFE])).unwrap();
    assert_eq!(sim.export_persistent_state(), retained);
    for mode in ["mcu", "power"] {
        sim.reset(mode).unwrap();
        assert_eq!(sim.export_persistent_state(), retained);
    }
    let mut other = WebSim::new();
    other.import_persistent_state(&retained).unwrap();
    assert_eq!(other.export_persistent_state(), retained);
    assert!(other.import_persistent_state("无效持久状态").is_err());
    assert_eq!(other.export_persistent_state(), retained);
}

#[test]
fn digits_are_low_active_and_expire_after_100ms() {
    let mut sim = WebSim::new();
    sim.load_hex(&hex(&[0x80, 0xFE])).unwrap();
    sim.sim.set_latch_mode("MM").unwrap();
    sim.sim.poke_xdata(0xC000, 1);
    sim.sim.poke_xdata(0xE000, 0xC0);
    sim.step();
    assert_eq!(snapshot(&sim)["digits"][0], 0xC0);
    sim.run_us(99_000);
    assert_eq!(snapshot(&sim)["digits"][0], 0xC0);
    sim.run_us(1_001);
    assert_eq!(snapshot(&sim)["digits"][0], Value::Null);
}

#[test]
fn validates_inputs_without_native_jsvalue_calls() {
    let mut sim = WebSim::new();
    for key in 4..=19 {
        sim.set_key(&format!("S{key}"), true).unwrap();
    }
    for key in ["S3", "S20", "", "4"] {
        assert!(sim.set_key(key, true).is_err());
    }
    for mode in ["KBD", "BTN"] {
        sim.set_key_mode(mode).unwrap();
        assert_eq!(
            sim.sim.snapshot().key_mode,
            stcjudge::KeyMode::parse(mode).unwrap()
        );
    }
    assert!(sim.set_key_mode("other").is_err());
    assert!(sim.reset("cpu").is_err());
    for channel in ["AIN1", "AIN3"] {
        sim.set_voltage(channel, 0.0).unwrap();
        sim.set_voltage(channel, 5.0).unwrap();
        assert!(sim.set_voltage(channel, -0.1).is_err());
        assert!(sim.set_voltage(channel, 5.1).is_err());
    }
    assert!(sim.set_voltage("AIN2", 2.5).is_err());
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(sim.set_temperature(value).is_err());
        assert!(sim.set_voltage("AIN1", value).is_err());
        assert!(sim.set_frequency(value).is_err());
        assert!(sim.set_distance(value).is_err());
    }
    sim.set_temperature(-55.0).unwrap();
    sim.set_temperature(125.0).unwrap();
    assert!(sim.set_temperature(125.1).is_err());
    assert!(sim.set_temperature(-55.1).is_err());
    assert!(sim.set_frequency(-1.0).is_err());
    assert!(sim.set_distance(-1.0).is_err());
    sim.set_frequency(0.0).unwrap();
    sim.set_distance(0.0).unwrap();
    sim.set_rtc(23, 59, 59).unwrap();
    for (h, m, s) in [(24, 0, 0), (0, 60, 0), (0, 0, 60), (256, 0, 0)] {
        assert!(sim.set_rtc(h, m, s).is_err());
    }
    for installed in [true, false] {
        sim.set_jumper(installed).unwrap();
        assert_eq!(sim.sim.snapshot().jumper_net_sig_to_sig_out, installed);
    }
}

#[test]
fn diagnostic_stops_execution_until_reset_or_load() {
    for single_step in [false, true] {
        let mut sim = WebSim::new();
        // 启动引擎明确不支持的 T0 模式 3，验证实际错误链而非手工注入诊断。
        sim.load_hex(&hex(&[0x75, 0x89, 0x03, 0x75, 0x88, 0x10, 0x80, 0xFE]))
            .unwrap();
        if single_step {
            for _ in 0..8 {
                sim.step();
            }
        } else {
            sim.run_us(100);
        }
        assert!(
            snapshot(&sim)["diagnostic"]
                .as_str()
                .unwrap()
                .contains("定时器0模式3")
        );
        let stopped = snapshot(&sim);
        sim.run_us(100);
        sim.step();
        assert_eq!(snapshot(&sim), stopped);
        assert!(sim.load_hex("错误 HEX").is_err());
        assert_eq!(snapshot(&sim), stopped);
        sim.reset("mcu").unwrap();
        assert_eq!(snapshot(&sim)["diagnostic"], Value::Null);
        sim.load_hex(&hex(&[0])).unwrap();
        assert_eq!(snapshot(&sim)["diagnostic"], Value::Null);
        sim.run_us(100);
        assert!(snapshot(&sim)["timeNs"].as_u64().unwrap() > 0);
    }
}

#[test]
fn uart_take_drains_output() {
    let mut sim = WebSim::new();
    sim.load_hex(&hex(&[0x80, 0xFE])).unwrap();
    // T2 的 1T 重装值 FEC7 产生约 9585 baud，落在默认 9600 baud 的接收容差内。
    sim.sim.poke_sfr(0xD6, 0xFE).unwrap();
    sim.sim.poke_sfr(0xD7, 0xC7).unwrap();
    sim.sim.poke_sfr(0x8E, 0x15).unwrap();
    sim.sim.poke_sfr(0x98, 0x50).unwrap();
    sim.sim.poke_sfr(0x99, b'A').unwrap();
    sim.run_us(2_000);
    assert_eq!(sim.uart_take().unwrap(), "A");
    assert_eq!(sim.uart_take().unwrap(), "");
    assert_eq!(sim.uart_take().unwrap(), "");
}

#[test]
fn uart_latin1_roundtrips_every_byte_without_utf8_encoding() {
    let mut sim = WebSim::new();
    sim.load_hex(&hex(&[0x80, 0xFE])).unwrap();
    // Existing UART frame model: STC15_DS.pdf UART1 chapter; T2 reload FEC7 at 12 MHz.
    sim.sim.poke_sfr(0xD6, 0xFE).unwrap();
    sim.sim.poke_sfr(0xD7, 0xC7).unwrap();
    sim.sim.poke_sfr(0x8E, 0x15).unwrap();
    sim.sim.poke_sfr(0x98, 0x50).unwrap();
    for byte in 0..=255_u8 {
        let text = char::from(byte).to_string();
        sim.uart_write(&text).unwrap();
        sim.run_us(2_000);
        assert_eq!(sim.sim.peek_sfr(0x99).unwrap(), byte, "RX {byte:02X}");
        assert_ne!(sim.sim.peek_sfr(0x98).unwrap() & 1, 0);
        sim.sim.poke_sfr(0x98, 0x50).unwrap();
        sim.sim.poke_sfr(0x99, byte).unwrap();
        sim.run_us(2_000);
        assert_eq!(sim.uart_take().unwrap(), text, "TX {byte:02X}");
        assert_eq!(sim.uart_take().unwrap(), "");
    }
}

#[test]
fn uart_rejects_non_byte_characters_before_queuing_any_input() {
    let mut sim = WebSim::new();
    assert!(sim.uart_write("A\u{100}").is_err());
    assert!(sim.uart_write("中文").is_err());
    assert!(sim.uart_write("😀").is_err());
}

#[test]
fn uart_byte_api_rejects_9bit_output_without_draining_it() {
    let mut sim = WebSim::new();
    sim.load_hex(&hex(&[0x80, 0xFE])).unwrap();
    sim.sim
        .configure_uart1(stcjudge::UartConfig {
            data_bits: 9,
            ..Default::default()
        })
        .unwrap();
    sim.sim.poke_sfr(0xD6, 0xFE).unwrap();
    sim.sim.poke_sfr(0xD7, 0xC7).unwrap();
    sim.sim.poke_sfr(0x8E, 0x15).unwrap();
    sim.sim.poke_sfr(0x98, 0xD8).unwrap();
    sim.sim.poke_sfr(0x99, 0xA5).unwrap();
    sim.run_us(2_000);
    assert_eq!(sim.sim.uart_peek_raw(), vec![0x1A5]);
    assert!(sim.uart_take().is_err());
    assert_eq!(sim.sim.uart_take_raw(), vec![0x1A5]);
}

#[test]
fn board_controls_validate_report_and_survive_loads_and_resets() {
    let mut sim = WebSim::new();
    let initial: Value = serde_json::from_str(&sim.board_state()).unwrap();
    assert_eq!(
        initial["jumpers"],
        json!({"J5":"KBD", "J13":"IO", "J6":"ON", "J2":"US", "J3":"NE555_OFF"})
    );
    assert_eq!(initial["dials"], json!({"RB2":0.0, "RB3":0.5}));
    for (id, position) in [
        ("J5", "BTN"),
        ("J13", "MM"),
        ("J6", "OFF"),
        ("J2", "IR"),
        ("J3", "NE555_ON"),
    ] {
        sim.set_board_jumper(id, position).unwrap();
    }
    sim.set_dial("RB2", 0.75).unwrap();
    sim.set_dial("RB3", 1.0).unwrap();
    sim.set_light(0.02).unwrap();
    sim.set_voltage("AIN1", 1.0).unwrap();
    sim.set_frequency(1234.0).unwrap();
    let before = sim.board_state();
    for (id, pos) in [
        ("J5", "IO"),
        ("J13", "BAD"),
        ("J6", "US"),
        ("J2", "ON"),
        ("J3", "ON"),
        ("J99", "ON"),
    ] {
        assert!(sim.set_board_jumper(id, pos).is_err());
    }
    for value in [-0.01, 1.01, f32::NAN, f32::INFINITY] {
        assert!(sim.set_dial("RB2", value).is_err());
        assert!(sim.set_dial("RB3", value).is_err());
        assert!(sim.set_light(value).is_err());
    }
    assert!(sim.set_dial("RB4", 0.5).is_err());
    assert_eq!(sim.board_state(), before);
    sim.load_hex(&hex(&[0x80, 0xFE])).unwrap();
    assert_eq!(sim.board_state(), before);
    for mode in ["mcu", "power"] {
        sim.reset(mode).unwrap();
        assert_eq!(sim.board_state(), before);
    }
    let state: Value = serde_json::from_str(&before).unwrap();
    assert_eq!(state["ain1V"], 1.0);
    assert_eq!(state["ain3V"], 3.75);
    assert_eq!(state["ne555Hz"], 1234.0);
    assert!((state["derived"]["ain1V"].as_f64().unwrap() - 2.857143).abs() < 0.0001);
    sim.set_key_mode("KBD").unwrap();
    sim.set_jumper(false).unwrap();
    let state: Value = serde_json::from_str(&sim.board_state()).unwrap();
    assert_eq!(state["jumpers"]["J5"], "KBD");
    assert_eq!(state["jumpers"]["J3"], "NE555_OFF");
}
