//! UART1 rates: STC15_DS.pdf pp.609–610,615,619–620,625–626.
//! Ideal noiseless midpoint receiver (p.617), independent clocks in each direction.
use super::*;

#[cfg(test)]
#[path = "uart_baud_tests.rs"]
mod tests;

#[derive(Debug, Default)]
pub(super) struct UartLine {
    pub(super) configured: bool,
    tx: Receiver,
    rx: Receiver,
    ti_sent: bool,
}

#[derive(Debug)]
struct Receiver {
    high: bool,
    sample_at: Option<u64>,
    started_at: u64,
    bit_ns: u64,
    index: u8,
    symbol: u16,
}

impl Default for Receiver {
    fn default() -> Self {
        Self {
            high: true,
            sample_at: None,
            started_at: 0,
            bit_ns: 1,
            index: 0,
            symbol: 0,
        }
    }
}

#[derive(Debug)]
struct Received {
    timed: TimedUartSymbol,
    stop: bool,
}

impl Receiver {
    // Consume [start,end). Carry sampling phase across CPU ticks and sender frames.
    fn segment(
        &mut self,
        high: bool,
        start: u64,
        end: u64,
        bit_ns: u64,
        bits: u8,
        out: &mut Vec<Received>,
    ) {
        let bit_ns = bit_ns.max(2);
        if let Some(at) = self.sample_at
            && self.bit_ns != bit_ns
        {
            self.sample_at = Some(
                start
                    + at.saturating_sub(start)
                        .saturating_mul(bit_ns)
                        .div_ceil(self.bit_ns),
            );
            self.bit_ns = bit_ns;
        }
        if self.sample_at.is_none() && self.high && !high {
            self.started_at = start;
            self.bit_ns = bit_ns.max(2);
            self.sample_at = Some(start + self.bit_ns / 2);
            self.index = 0;
            self.symbol = 0;
        }
        self.high = high;
        self.samples(high, end, false, bits, out);
    }

    fn boundary(&mut self, high: bool, at: u64, bit_ns: u64, bits: u8, out: &mut Vec<Received>) {
        // Resolve the sender's transition first, then samples exactly at the CPU boundary.
        self.segment(high, at, at, bit_ns, bits, out);
        self.samples(high, at, true, bits, out);
    }

    fn samples(
        &mut self,
        high: bool,
        end: u64,
        inclusive: bool,
        bits: u8,
        out: &mut Vec<Received>,
    ) {
        while let Some(at) = self
            .sample_at
            .filter(|at| *at < end || inclusive && *at == end)
        {
            if self.index == 0 && high {
                self.sample_at = None; // False start.
                break;
            }
            if self.index > bits {
                out.push(Received {
                    timed: TimedUartSymbol {
                        symbol: self.symbol,
                        started_at_ns: self.started_at,
                        completed_at_ns: at,
                    },
                    stop: high,
                });
                self.sample_at = None;
                break;
            }
            if self.index > 0 && high {
                self.symbol |= 1 << (self.index - 1);
            }
            self.index += 1;
            self.sample_at = Some(at.saturating_add(self.bit_ns));
        }
    }
}

impl Simulator {
    // UART uses the selected IRC after CLK_DIV, shared with CPU and timers.
    fn uart_clock_hz(&self) -> u64 {
        self.sysclk_hz()
    }

    fn uart1_bit_ticks(&self) -> Option<u64> {
        let scon = self.ctx.ports.uart1.control;
        let auxr = self.cpu.sfr(SFR_AUXR, &self.ctx);
        let smod = self.cpu.sfr(SFR_PCON, &self.ctx) & 0x80 != 0;
        match scon >> 6 {
            0 => Some(if auxr & AUXR_UART_M0_X6 != 0 { 2 } else { 12 }),
            2 => Some(if smod { 32 } else { 64 }),
            _ => self.ctx.ports.timers.uart1_bit_ticks(auxr, smod),
        }
    }

    pub fn uart1_firmware_baud(&self) -> Option<f64> {
        // Reset SCON=0 is not a configured asynchronous UART.
        if !self.ctx.ports.uart1.line.configured {
            return None;
        }
        self.uart1_bit_ticks()
            .map(|ticks| self.uart_clock_hz() as f64 / ticks as f64)
    }

    pub fn set_uart_terminal_baud(&mut self, baud: u32) -> Result<()> {
        let config = UartConfig {
            baud_rate: baud,
            ..self.ctx.ports.uart1.config
        };
        self.ctx.ports.uart1.configure(config)
    }

    pub fn uart_terminal_baud(&self) -> u32 {
        self.ctx.ports.uart1.config.baud_rate
    }

    pub(super) fn tick_uart1(&mut self, start: u64, elapsed: u64) {
        let bit_ns = self
            .uart1_bit_ticks()
            .map(|ticks| (ticks * NS_PER_SECOND).div_ceil(self.uart_clock_hz()));
        let uart = &mut self.ctx.ports.uart1;
        // Mode 0 is synchronous, not an asynchronous 8N1 terminal link.
        if uart.control >> 6 == 0 {
            uart.tick_sync(start, elapsed, bit_ns.unwrap());
            return;
        }
        uart.tick_line(start, elapsed, bit_ns);
    }
}

fn timed_frame(symbol: u16, config: UartConfig, start: u64, bit_ns: u64) -> UartFrame {
    let mut frame = UartFrame::new(symbol, config, start);
    let original = config.bit_ns();
    for segment in &mut frame.segments {
        segment.duration_ns = segment.duration_ns * bit_ns / original;
    }
    frame.clock_divider = bit_ns;
    frame.segment_remaining_ns = bit_ns;
    frame.completed_at_ns = start + frame.segments.iter().map(|s| s.duration_ns).sum::<u64>();
    frame
}

// Feed every bit transition, even when a CPU instruction spans multiple bits.
fn sample(
    frame: &mut UartFrame,
    receiver: &mut Receiver,
    start: u64,
    end: u64,
    bit_ns: u64,
    bits: u8,
    out: &mut Vec<Received>,
) -> bool {
    let mut now = start;
    while now < end {
        let until = end.min(now.saturating_add(frame.segment_remaining_ns));
        receiver.segment(frame.current_level(), now, until, bit_ns, bits, out);
        if frame.advance(until - now) {
            return true;
        }
        now = until;
    }
    false
}

impl Uart {
    fn tick_sync(&mut self, start: u64, elapsed: u64, bit_ns: u64) {
        // p.615–616: TxD is shift clock, not data. The PC sees eight clock pulses.
        if self.tx_frame.is_none()
            && let Some(symbol) = self.tx_pending.pop_front()
        {
            let mut frame = timed_frame(symbol, self.config, start, bit_ns);
            frame.segments = (0..16)
                .map(|i| UartFrameSegment {
                    level: i % 2 != 0,
                    duration_ns: bit_ns / 2,
                })
                .collect();
            frame.segment_remaining_ns = bit_ns / 2;
            frame.completed_at_ns = start + (bit_ns / 2) * 16;
            self.tx_frame = Some(frame);
        }
        let mut out = Vec::new();
        let end = start + elapsed;
        if let Some(frame) = self.tx_frame.as_mut() {
            frame.set_clock_divider(bit_ns, start);
            let until = end.min(frame.completed_at_ns);
            let done = sample(
                frame,
                &mut self.line.tx,
                start,
                until,
                self.config.bit_ns(),
                self.config.data_bits,
                &mut out,
            );
            self.tx_line_high = if done { true } else { frame.current_level() };
            if done {
                self.tx_frame = None;
                let old = self.control;
                self.control |= SCON_TI;
                self.note_interrupt_flag_edges(old, self.control);
                self.line.tx.segment(
                    true,
                    until,
                    end,
                    self.config.bit_ns(),
                    self.config.data_bits,
                    &mut out,
                );
            }
        } else {
            self.line.tx.segment(
                true,
                start,
                end,
                self.config.bit_ns(),
                self.config.data_bits,
                &mut out,
            );
        }
        self.tx_queue.extend(out.into_iter().map(|r| r.timed));
        // No synchronous external peer is connected to the web's asynchronous terminal.
        self.rx_queue.clear();
        self.rx_frame = None;
        self.line.rx = Receiver::default();
        self.rx_line_high = true;
    }

    fn tick_line(&mut self, start: u64, elapsed: u64, fw_bit_ns: Option<u64>) {
        let end = start.saturating_add(elapsed);
        let fw_bits = if self.control & 0x80 != 0 { 9 } else { 8 };
        let fw_config = UartConfig {
            data_bits: fw_bits,
            parity: UartParity::None,
            stop_bits: UartStopBits::One,
            ..self.config
        };
        let mut decoded = Vec::new();
        let mut now = start;
        if let Some(bit_ns) = fw_bit_ns {
            while now <= end {
                if self.tx_frame.is_none() {
                    if let Some(symbol) = self.tx_pending.pop_front() {
                        self.tx_frame = Some(timed_frame(symbol, fw_config, now, bit_ns));
                        self.line.ti_sent = false;
                        let track = self.event_track.track_id();
                        self.push_event_note(now, || {
                            WaveEventNote::with_detail(
                                now,
                                track,
                                uart_event_label("TX", symbol),
                                format!("bits={fw_bits}"),
                            )
                        });
                    } else {
                        self.line.tx.segment(
                            true,
                            now,
                            end,
                            self.config.bit_ns(),
                            self.config.data_bits,
                            &mut decoded,
                        );
                        break;
                    }
                }
                let frame = self.tx_frame.as_mut().unwrap();
                frame.set_clock_divider(bit_ns, now);
                let until = end.min(frame.completed_at_ns);
                let stop_start = frame.completed_at_ns - bit_ns;
                let done = sample(
                    frame,
                    &mut self.line.tx,
                    now,
                    until,
                    self.config.bit_ns(),
                    self.config.data_bits,
                    &mut decoded,
                );
                self.tx_line_high = if done { true } else { frame.current_level() };
                if !self.line.ti_sent && until >= stop_start {
                    self.line.ti_sent = true;
                    let old = self.control;
                    self.control |= SCON_TI;
                    self.note_interrupt_flag_edges(old, self.control);
                }
                if done {
                    self.tx_frame = None;
                } else {
                    break;
                }
                now = until;
            }
        } else {
            // A stopped generator holds the pin and the unsent phase, not CPU time.
            if let Some(frame) = self.tx_frame.as_mut() {
                frame.completed_at_ns = frame.completed_at_ns.saturating_add(elapsed);
            }
            self.line.tx.segment(
                self.tx_line_high,
                start,
                end,
                self.config.bit_ns(),
                self.config.data_bits,
                &mut decoded,
            );
        }
        self.line.tx.boundary(
            self.tx_line_high,
            end,
            self.config.bit_ns(),
            self.config.data_bits,
            &mut decoded,
        );
        self.tx_queue.extend(decoded.drain(..).map(|r| r.timed));
        let rx_enabled = fw_bit_ns.is_some() && self.control & SCON_REN != 0;
        if !rx_enabled {
            self.line.rx = Receiver::default();
        }
        now = start;
        while now <= end {
            if self.rx_frame.is_none() {
                if let Some(symbol) = self.rx_queue.pop_front() {
                    self.rx_frame = Some(UartFrame::new(symbol, self.config, now));
                    let track = self.event_track.track_id();
                    self.push_event_note(now, || {
                        WaveEventNote::with_detail(
                            now,
                            track,
                            uart_event_label("RX", symbol),
                            format!("bits={fw_bits}"),
                        )
                    });
                } else {
                    if rx_enabled && let Some(bit_ns) = fw_bit_ns {
                        self.line
                            .rx
                            .segment(true, now, end, bit_ns, fw_bits, &mut decoded);
                    }
                    break;
                }
            }
            let frame = self.rx_frame.as_mut().unwrap();
            let until = end.min(frame.completed_at_ns);
            let done = if rx_enabled {
                sample(
                    frame,
                    &mut self.line.rx,
                    now,
                    until,
                    fw_bit_ns.unwrap(),
                    fw_bits,
                    &mut decoded,
                )
            } else {
                frame.advance(until - now)
            };
            self.rx_line_high = if done { true } else { frame.current_level() };
            if done {
                self.rx_frame = None;
            } else {
                break;
            }
            now = until;
        }
        if rx_enabled {
            self.line.rx.boundary(
                self.rx_line_high,
                end,
                fw_bit_ns.unwrap(),
                fw_bits,
                &mut decoded,
            );
        }
        for received in decoded {
            // pp.617,620: do not overwrite unread SBUF; SM2 filters stop/address bit.
            let rb8 = if fw_bits == 9 {
                received.timed.symbol & 0x100 != 0
            } else {
                received.stop
            };
            if self.control & (SCON_REN | SCON_RI) == SCON_REN && (self.control & 0x20 == 0 || rb8)
            {
                let old = self.control;
                self.rx_sbuf = received.timed.symbol as u8;
                self.set_rb8(rb8);
                self.control |= SCON_RI;
                self.note_interrupt_flag_edges(old, self.control);
            }
        }
    }
}
