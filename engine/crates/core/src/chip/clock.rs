//! 主时钟与 SYSclk 的关系见手册 p.206。
use super::UartFrame;

pub(super) fn divider(clk_div: u8) -> u64 {
    1 << (clk_div & 7)
}

impl UartFrame {
    pub(super) fn set_clock_divider(&mut self, divider: u64, now_ns: u64) {
        if self.clock_divider == divider {
            return;
        }
        let scale = |ns: u64| ns.saturating_mul(divider).div_ceil(self.clock_divider);
        self.segment_remaining_ns = scale(self.segment_remaining_ns);
        for segment in &mut self.segments {
            segment.duration_ns = scale(segment.duration_ns);
        }
        self.completed_at_ns = self.segments[self.segment_index + 1..].iter().fold(
            now_ns.saturating_add(self.segment_remaining_ns),
            |time_ns, segment| time_ns.saturating_add(segment.duration_ns),
        );
        self.clock_divider = divider;
    }
}
