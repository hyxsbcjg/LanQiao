/// Fractions of virtual time with each physical LED conducting.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Brightness {
    pub window_ns: u64,
    pub seg: [[f32; 8]; 8],
    pub led: [f32; 8],
}

#[derive(Debug, Default)]
pub(crate) struct LitTime {
    start_ns: u64,
    last_ns: u64,
    // Raw LED, COM, SEG Q banks, independent of the judge's retained digits.
    latches: [u8; 3],
    seg_ns: [[u64; 8]; 8],
    led_ns: [u64; 8],
}

impl LitTime {
    #[inline]
    pub(crate) fn sample(&mut self, latches: &[u8; 4], time_ns: u64) {
        let next = [latches[0], latches[2], latches[3]];
        if next != self.latches {
            self.flush(time_ns);
            self.latches = next;
        }
    }

    fn flush(&mut self, time_ns: u64) {
        let elapsed = time_ns.saturating_sub(self.last_ns);
        self.last_ns = time_ns;
        if elapsed == 0 {
            return;
        }
        // SCH_V31.pdf p3: U6 Q=0 lights L1..8; U8 Q=1 selects COM;
        // U7 Q=0 lights a..dp. 74HC_HCT573.pdf p1, p6 Table 4: held Q
        // continues conducting even after the judge's 100ms sample expires.
        for digit in 0..8 {
            if self.latches[1] & (1 << digit) != 0 {
                for segment in 0..8 {
                    if self.latches[2] & (1 << segment) == 0 {
                        self.seg_ns[digit][segment] += elapsed;
                    }
                }
            }
            if self.latches[0] & (1 << digit) == 0 {
                self.led_ns[digit] += elapsed;
            }
        }
    }

    pub(crate) fn clear(&mut self, time_ns: u64) {
        self.start_ns = time_ns;
        self.last_ns = time_ns;
        self.seg_ns = [[0; 8]; 8];
        self.led_ns = [0; 8];
    }

    pub(crate) fn take(&mut self, time_ns: u64) -> Brightness {
        self.flush(time_ns);
        let window_ns = time_ns.saturating_sub(self.start_ns);
        let fraction = |ns: u64| {
            if window_ns == 0 {
                0.0
            } else {
                (ns as f64 / window_ns as f64) as f32
            }
        };
        let value = Brightness {
            window_ns,
            seg: self.seg_ns.map(|digit| digit.map(fraction)),
            led: self.led_ns.map(fraction),
        };
        self.clear(time_ns);
        value
    }
}
