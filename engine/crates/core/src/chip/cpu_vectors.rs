//! 从 MIT 参考模型 core_test.c 移植；期望按本仓库手册 §5.3 推导。
use super::*;

fn machine(code: &[u8]) -> Simulator {
    Simulator::from_code_with_options(code.to_vec(), false, WaveCaptureOptions::default())
}

fn steps(sim: &mut Simulator, count: usize) {
    for _ in 0..count {
        sim.step().expect("指令执行成功");
    }
}

#[test]
fn exhaustive_add_addc_subb_flags() {
    let mut cases = 0;
    for (opcode, subtract, carries) in [(0x24, false, 1), (0x34, false, 2), (0x94, true, 2)] {
        let mut sim = machine(&[opcode, 0]);
        for carry in 0..carries {
            for a in 0..=255_u16 {
                for b in 0..=255_u16 {
                    sim.ctx.code.code[1] = b as u8;
                    sim.cpu.pc = 0;
                    sim.cpu.register_set(Register::A, a);
                    sim.cpu.register_set(Register::PSW, carry << 7);
                    sim.step().unwrap();
                    let value = if subtract {
                        a as i32 - b as i32 - carry as i32
                    } else {
                        a as i32 + b as i32 + carry as i32
                    };
                    let result = value as u8;
                    let cy = if subtract { value < 0 } else { value > 255 };
                    let ac = if subtract {
                        (a & 15) < (b & 15) + carry
                    } else {
                        (a & 15) + (b & 15) + carry > 15
                    };
                    let ov = if subtract {
                        (a ^ b) & (a ^ u16::from(result)) & 128 != 0
                    } else {
                        (!(a ^ b)) & (a ^ u16::from(result)) & 128 != 0
                    };
                    let flags = (u16::from(cy) << 7)
                        | (u16::from(ac) << 6)
                        | (u16::from(ov) << 2)
                        | (result.count_ones() as u16 & 1);
                    assert_eq!(
                        (sim.cpu.a(), sim.cpu.register(Register::PSW) & 0xc5),
                        (result, flags),
                        "op={opcode:02X}, A={a:02X}, src={b:02X}, CY={carry}"
                    );
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 327_680);
}

#[test]
fn upper_iram_stack_and_indirect_are_not_sfrs() {
    let mut sim = machine(&[
        0x75, 0x81, 0x7f, 0x74, 0x5a, 0xc0, 0xe0, 0xe4, 0xd0, 0xe0, 0x78, 0x90, 0x76, 0x55, 0xe6,
        0xf5, 0x30,
    ]);
    steps(&mut sim, 9);
    assert_eq!(sim.cpu.internal_ram[0x80], 0x5a);
    assert_eq!(sim.cpu.internal_ram[0x90], 0x55);
    assert_eq!(sim.ctx.ports.port_latch[..2], [0xff, 0xff]);
    assert_eq!(sim.cpu.sp(), 0x7f);
    assert_eq!(sim.cpu.internal_ram[0x30], 0x55);
}

#[test]
fn mov_86_87_direction_and_xchd_low_nibbles() {
    for ri in 0..=1_u8 {
        let mut sim = machine(&[0x86 + ri, 0x30, 0xd6 + ri]);
        sim.cpu.internal_ram[usize::from(ri)] = 0x40;
        sim.cpu.internal_ram[0x40] = 0xcd;
        steps(&mut sim, 1);
        assert_eq!(sim.cpu.internal_ram[0x30], 0xcd);
        sim.cpu.a_set(0xab);
        steps(&mut sim, 1);
        assert_eq!(sim.cpu.a(), 0xad);
        assert_eq!(sim.cpu.internal_ram[0x40], 0xcb);
    }
}

#[test]
fn register_banks_bit_alias_and_direct_mov_order() {
    for bank in 0..4 {
        let mut sim = machine(&[0x78, 0x22, 0xd2, 0x00, 0x85, 0x30, 0x31]);
        sim.cpu.register_set(Register::PSW, bank << 3);
        sim.cpu.internal_ram[0x30] = 0x12;
        sim.cpu.internal_ram[0x31] = 0x34;
        steps(&mut sim, 3);
        assert_eq!(sim.cpu.internal_ram[bank as usize * 8], 0x22);
        assert_eq!(sim.cpu.internal_ram[0x20], 1);
        assert_eq!(sim.cpu.internal_ram[0x31], 0x12);
    }
}

#[test]
fn dual_dptr_and_movx_internal_short_external_page() {
    let mut sim = machine(&[
        0x90, 1, 0x23, 0x75, 0xa2, 1, 0x90, 4, 0x56, 0x75, 0xa2, 0, 0xa3, 0x75, 0xa2, 1, 0xf0,
        0xe0, 0xf2, 0xe2, 0xf3, 0xe3,
    ]);
    steps(&mut sim, 5);
    assert_eq!(sim.cpu.dptr(), 0x124);
    steps(&mut sim, 1);
    assert_eq!(sim.cpu.dptr(), 0x456);
    sim.cpu.a_set(0x7a);
    steps(&mut sim, 1);
    assert_eq!(sim.ctx.xdata.raw_read(0x456), 0x7a);
    sim.cpu.a_set(0);
    steps(&mut sim, 1);
    assert_eq!(sim.cpu.a(), 0x7a);
    sim.cpu.internal_ram[0] = 0xff;
    sim.cpu.internal_ram[1] = 0xfe;
    sim.cpu.sfr_set(SFR_P2, 0xa0, &mut sim.ctx);
    for (pc, addr) in [(18, 0xff), (20, 0xfe)] {
        sim.cpu.pc = pc;
        sim.cpu.a_set(0x7a);
        steps(&mut sim, 1);
        assert_eq!(sim.ctx.xdata.raw_read(addr), 0x7a);
        sim.cpu.a_set(0);
        steps(&mut sim, 1);
        assert_eq!(sim.cpu.a(), 0x7a);
    }
    sim.cpu.sfr_set(SFR_AUXR, AUXR_EXTRAM, &mut sim.ctx);
    sim.cpu.pc = 18;
    sim.cpu.a_set(0x69);
    steps(&mut sim, 1);
    assert_eq!(sim.ctx.xdata.raw_read(0xa0ff), 0x69);
    assert_eq!(sim.ctx.xdata.raw_read(0xff), 0x7a);
}

#[test]
fn movc_pc_and_dptr_use_code_space() {
    let mut sim = machine(&[0x74, 1, 0x83, 0xaa, 0xbb]);
    steps(&mut sim, 2);
    assert_eq!((sim.cpu.a(), sim.cpu.pc), (0xbb, 3));
    let mut code = vec![0; 0x102];
    code[0] = 0x93;
    code[0x100] = 0x5a;
    let mut sim = machine(&code);
    sim.cpu.dptr_set(0xff);
    sim.cpu.a_set(1);
    sim.ctx.xdata.raw_write(0x100, 0x11);
    steps(&mut sim, 1);
    assert_eq!((sim.cpu.a(), sim.cpu.dptr()), (0x5a, 0xff));
}

#[test]
fn call_return_stack_order_wrap_and_pop_sp() {
    let mut code = vec![0; 0x20];
    code[..3].copy_from_slice(&[0x12, 0, 0x10]);
    code[0x10] = 0x22;
    let mut sim = machine(&code);
    sim.cpu.sp_set(0xff);
    steps(&mut sim, 1);
    assert_eq!(
        (
            sim.cpu.sp(),
            sim.cpu.internal_ram[0],
            sim.cpu.internal_ram[1]
        ),
        (1, 3, 0)
    );
    steps(&mut sim, 1);
    assert_eq!((sim.cpu.pc, sim.cpu.sp()), (3, 0xff));
    let mut sim = machine(&[0xd0, 0x81]);
    sim.cpu.sp_set(0x80);
    sim.cpu.internal_ram[0x80] = 0x42;
    steps(&mut sim, 1);
    assert_eq!(sim.cpu.sp(), 0x42);
}

#[test]
fn arithmetic_non_add_vectors_preserve_the_documented_flags() {
    let mut sim = machine(&[0x04, 0xb4, 3, 2]);
    sim.cpu.a_set(0xff);
    sim.cpu.psw_set(Flag::C, true);
    steps(&mut sim, 1);
    assert_eq!(sim.cpu.a(), 0);
    assert!(sim.cpu.psw(Flag::C));
    sim.cpu.a_set(2);
    steps(&mut sim, 1);
    assert_eq!(sim.cpu.pc, 6);
    assert!(sim.cpu.psw(Flag::C));

    for (opcode, a, b, result, remainder, overflow) in
        [(0xa4, 0x10, 0x10, 0, 1, true), (0x84, 7, 3, 2, 1, false)]
    {
        let mut sim = machine(&[opcode]);
        sim.cpu.a_set(a);
        sim.cpu.b_set(b);
        sim.cpu.psw_set(Flag::C, true);
        steps(&mut sim, 1);
        assert_eq!((sim.cpu.a(), sim.cpu.b()), (result, remainder));
        assert!(!sim.cpu.psw(Flag::C));
        assert_eq!(sim.cpu.psw(Flag::OV), overflow);
    }
    let mut sim = machine(&[0x84]);
    sim.cpu.a_set(7);
    steps(&mut sim, 1);
    assert!(sim.cpu.psw(Flag::OV));
    assert!(!sim.cpu.psw(Flag::C));
    let mut sim = machine(&[0x74, 9, 0x24, 1, 0xd4]);
    steps(&mut sim, 3);
    assert_eq!(sim.cpu.a(), 0x10);
    let mut sim = machine(&[0x33, 0x13]);
    sim.cpu.a_set(0x80);
    sim.cpu.psw_set(Flag::C, true);
    steps(&mut sim, 1);
    assert_eq!(sim.cpu.a(), 1);
    assert!(sim.cpu.psw(Flag::C));
    steps(&mut sim, 1);
    assert_eq!(sim.cpu.a(), 0x80);
    assert!(sim.cpu.psw(Flag::C));
}

#[test]
fn every_defined_opcode_dispatches_through_simulator() {
    for opcode in (0..=255_u8).filter(|&op| op != 0xa5) {
        let mut sim = machine(&[opcode, 0, 0]);
        sim.step()
            .unwrap_or_else(|err| panic!("op={opcode:02X}: {err}"));
    }
}

#[test]
fn parity_follows_direct_acc_writes_and_psw_writes() {
    // 手册 p.306：P 始终反映 A 中 1 的奇偶性，包括 direct/bit 写 ACC。
    let mut sim = machine(&[0x75, 0xe0, 1, 0x75, 0xd0, 0, 0xb2, 0xe1, 0x05, 0xe0]);
    for (a, parity) in [(1, 1), (1, 1), (3, 0), (4, 1)] {
        steps(&mut sim, 1);
        assert_eq!(
            (sim.cpu.a(), sim.cpu.register(Register::PSW) & 1),
            (a, parity)
        );
    }
}

#[test]
fn ajmp_acall_use_the_next_pc_page() {
    // 手册 pp.359、362：页号取 PC+2，涵盖页边界前/边界/边界后。
    for pc in [0x07fd, 0x07fe, 0x07ff, 0xfffe] {
        for opcode in [0x01, 0x11] {
            let mut code = vec![0; 0x10000];
            code[pc] = opcode;
            code[pc + 1] = 0x42;
            let mut sim = machine(&code);
            sim.cpu.pc = pc as u16;
            let next = (pc as u16).wrapping_add(2);
            let target = (next & 0xf800) | 0x42;
            match sim.cpu.decode_pc(&sim.ctx).control_flow() {
                i8051::ControlFlow::Continue(addr) if opcode == 0x01 => assert_eq!(addr, target),
                i8051::ControlFlow::Call(ret, addr) if opcode == 0x11 => {
                    assert_eq!((ret, addr), (next, target));
                }
                _ => panic!("跳转解码类型错误"),
            }
            steps(&mut sim, 1);
            assert_eq!(sim.cpu.pc, target);
            if opcode == 0x11 {
                assert_eq!(sim.cpu.sp(), 9);
                assert_eq!(&sim.cpu.internal_ram[8..10], &next.to_le_bytes());
            }
        }
    }
}

#[test]
fn gpio_rmw_reads_latch_but_mov_and_jb_read_pins() {
    // 手册 §5.3：外部拉低不改变锁存器；JBC/CPL/DJNZ 使用锁存值。
    let mut sim = machine(&[
        0xe5, 0xa0, 0xf5, 0x30, 0x53, 0xa0, 0xfe, 0xe5, 0xa0, 0x10, 0xa1, 2, 0x74, 0xff, 0xb2, 0xa1,
    ]);
    sim.ctx.board.pcf8591.force_lines_for_test(false, true);
    steps(&mut sim, 4);
    assert_eq!(sim.cpu.internal_ram[0x30] & 3, 1);
    assert_eq!(sim.ctx.ports.port_latch[2], 0xfe);
    assert_eq!(sim.cpu.a() & 3, 0);
    steps(&mut sim, 1);
    assert_eq!(sim.cpu.pc, 14, "JBC 读锁存器，外部拉低不应阻止跳转");
    assert_eq!(sim.ctx.ports.port_latch[2], 0xfc);
    steps(&mut sim, 1);
    assert_eq!(sim.ctx.ports.port_latch[2], 0xfe);

    let mut sim = machine(&[0x75, 0xa0, 0xff, 0xb2, 0xa1]);
    sim.ctx.board.pcf8591.force_lines_for_test(false, true);
    steps(&mut sim, 2);
    assert_eq!(sim.ctx.ports.port_latch[2], 0xfd, "CPL 反转锁存位");

    let mut sim = machine(&[0x75, 0xa0, 3, 0xd5, 0xa0, 2]);
    sim.ctx.board.pcf8591.force_lines_for_test(false, true);
    steps(&mut sim, 2);
    assert_eq!(sim.ctx.ports.port_latch[2], 2);
    assert_eq!(sim.cpu.pc, 8, "DJNZ 按递减后的锁存值判断，不能按引脚判断");

    for opcode in [0x20, 0x30] {
        let mut sim = machine(&[opcode, 0xa1, 2]);
        sim.ctx.board.pcf8591.force_lines_for_test(false, true);
        steps(&mut sim, 1);
        assert_eq!(sim.cpu.pc, if opcode == 0x20 { 3 } else { 5 });
    }
}

#[test]
fn high_priority_interrupt_preempts_low_but_not_its_peer() {
    // 手册 pp.442、447：T1 高优先级可抢占 T0；RETI 只弹出当前服务级。
    let mut code = vec![0; 0x30];
    code[0x0c] = 0x32;
    code[0x1b] = 0x32;
    let mut sim = machine(&code);
    sim.cpu.ie_set(IE_EA | IE_ET0 | IE_ET1);
    sim.cpu.ip_set(IE_ET1);
    sim.cpu.sfr_set(SFR_TCON, TCON_TF0, &mut sim.ctx);
    steps(&mut sim, 1);
    assert_eq!((sim.cpu.pc, sim.cpu.sp()), (0x0b, 9));
    sim.cpu.sfr_set(SFR_TCON, TCON_TF1, &mut sim.ctx);
    steps(&mut sim, 1);
    assert_eq!((sim.cpu.pc, sim.cpu.sp()), (0x1b, 11));
    steps(&mut sim, 1);
    assert_eq!((sim.cpu.pc, sim.cpu.sp()), (0x0b, 9));
    sim.cpu.sfr_set(SFR_TCON, TCON_TF0, &mut sim.ctx);
    steps(&mut sim, 1);
    assert_eq!(sim.cpu.pc, 0x0c, "RETI 后执行一条指令，同级不能嵌套");
    steps(&mut sim, 1);
    assert_eq!((sim.cpu.pc, sim.cpu.sp()), (0, 7));
    steps(&mut sim, 1);
    assert_eq!(sim.cpu.pc, 1);
    steps(&mut sim, 1);
    assert_eq!(sim.cpu.pc, 0x0b);
}

#[test]
fn interrupt_service_priority_is_latched_and_ret_does_not_release_it() {
    let mut code = vec![0; 0x30];
    code[0x0b] = 0x22;
    let mut sim = machine(&code);
    sim.cpu.ie_set(IE_EA | IE_ET0 | IE_ET1);
    sim.cpu.ip_set(IE_ET0);
    sim.cpu.sfr_set(SFR_TCON, TCON_TF0, &mut sim.ctx);
    steps(&mut sim, 1);
    sim.cpu.ip_set(IE_ET1);
    sim.cpu.sfr_set(SFR_TCON, TCON_TF1, &mut sim.ctx);
    steps(&mut sim, 1);
    assert_eq!((sim.cpu.pc, sim.cpu.sp()), (0, 7), "RET 只弹出 PC");
    steps(&mut sim, 1);
    assert_eq!(sim.cpu.pc, 1, "修改 IP 或使用 RET 都不能释放高优先级服务锁");
}
