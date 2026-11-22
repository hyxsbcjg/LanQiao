use super::BOARD_POWER_ON_LATCHES;

/// V2 board connections: J13 1=GND, 2=WR, 3=P42/WR; U24/U25 and U6-U9.
/// 74HC_HCT573.pdf p.6 Table 4: LE = !(Yn | WR), transparent while LE is high.
#[derive(Clone, Copy)]
pub(super) struct BoardLatches {
    pub values: [u8; 4],
    pub versions: [u64; 4],
    pub mm: bool,
    pub source: &'static str,
    pub pins: [u8; 6],
}

impl Default for BoardLatches {
    fn default() -> Self {
        Self {
            values: BOARD_POWER_ON_LATCHES,
            versions: [0; 4],
            mm: false,
            source: "none",
            pins: [0xFF; 6],
        }
    }
}

impl BoardLatches {
    pub fn sample(&mut self, select: u8, data: u8, wr_high: bool, source: &'static str) {
        if wr_high || select & 0x80 == 0 {
            return;
        }
        let slot = usize::from((select >> 5) - 4);
        self.values[slot] = data;
        self.versions[slot] = self.versions[slot].saturating_add(1);
        self.source = source;
    }

    pub fn sample_gpio(&mut self) {
        self.sample(
            self.pins[2],
            self.pins[0],
            self.mm && self.pins[4] & 4 != 0,
            "p0_p2",
        );
    }

    pub fn external_write(&mut self, addr: u16, data: u8) {
        // External MOVX drives P2=A15..A8 and P0=data with /WR low.
        // In IO, /WR is grounded even during the address phase. Only final Q is
        // summarized here; sub-instruction address/data transients are not timed.
        self.sample((addr >> 8) as u8, data, false, "xdata");
        // Bus release restores GPIO. In MM a high P4.2 closes all four latches.
        self.sample_gpio();
    }
}
