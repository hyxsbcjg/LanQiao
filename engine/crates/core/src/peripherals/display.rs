use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use super::{LitTime, RelayEvents};
use anyhow::{Result, bail};

const DIGIT_REFRESH_TIMEOUT_NS: u64 = 100_000_000;
// ponytail: unpaired latch changes use a dwell filter with a 1us floor; integrate segment
// on-time instead if short unpaired pulses or PWM brightness must be represented.
// This is an observation policy, not the 74HC573 propagation delay.
const DIGIT_STABLE_NS: u64 = 1_000;

#[derive(Debug, Default)]
pub(crate) struct Outputs {
    pub(crate) brightness: LitTime,
    pub(crate) leds: [bool; 8],
    pub(crate) relay_on: bool,
    pub(crate) relay_events: RefCell<RelayEvents>,
    pub(crate) motor_on: bool,
    pub(crate) buzzer_on: bool,
    pub(crate) digits: [DigitSample; 8],
    digit_refresh_ns: [u64; 8],
    next_digit_expire_ns: u64,
    last_digits_change_ns: u64,
    segment_latch: u8,
    com_latch: u8,
    latch_stable_since_ns: u64,
    last_com_change_ns: u64,
    com_dwell_ns: u64,
    last_digit_strobe_ns: u64,
    pending_digit_refresh_ns: Option<u64>,
    pending_digit_stable_ns: u64,
    pending_com_strobe: bool,
    pending_seg_strobe: bool,
    digit_transition: Option<DigitTransition>,
    last_com_strobe: u64,
    last_seg_strobe: u64,
}

impl Outputs {
    pub(crate) fn sample_from_latches(
        &mut self,
        latches: &[u8; 4],
        versions: &[u64; 4],
        time_ns: u64,
    ) {
        self.brightness.sample(latches, time_ns);
        let led = latches[0];
        for bit in 0..8 {
            self.leds[bit] = led & (1 << bit) == 0;
        }

        let ctrl = latches[1];
        // SCH_V31 p3 / V2 U9 Q4 -> U10 I5/O5 -> K1; 74HC573 p6 Table 4.
        // Observe raw drive separately: BoardModel subsequently gates relay_on by VB.
        self.relay_events
            .get_mut()
            .sample(ctrl & (1 << 4) != 0, time_ns);
        self.relay_on = ctrl & (1 << 4) != 0;
        self.motor_on = ctrl & (1 << 5) != 0;
        self.buzzer_on = ctrl & (1 << 6) != 0;

        // Finish the previous dwell before applying a new latch output.
        if self.pending_digit_refresh_ns.is_some()
            && time_ns.saturating_sub(self.latch_stable_since_ns)
                >= self.pending_digit_stable_ns.max(DIGIT_STABLE_NS)
        {
            self.sample_digits();
        }
        // A completed dwell retires pairing hints. Otherwise the next frame's
        // SEG-first update could consume a stale COM request from a blank slot.
        if time_ns.saturating_sub(self.last_digit_strobe_ns)
            >= (self.com_dwell_ns / 2).max(DIGIT_STABLE_NS)
        {
            self.pending_com_strobe = false;
            self.pending_seg_strobe = false;
        }

        let com_strobed = versions[2] != self.last_com_strobe;
        let seg_strobed = versions[3] != self.last_seg_strobe;
        if com_strobed || seg_strobed {
            self.last_digit_strobe_ns = time_ns;
            // 74HC_HCT573.pdf p1, p6 Table 4: independent Q banks hold data.
            // SCH_V31.pdf p3: U8 COM and U7 SEG jointly drive the common-anode LEDs.
            let com_changed = self.com_latch != latches[2];
            let seg_changed = self.segment_latch != latches[3];
            if com_changed {
                if self.com_latch != 0 {
                    self.com_dwell_ns = time_ns.saturating_sub(self.last_com_change_ns);
                }
                self.last_com_change_ns = time_ns;
            }
            if com_changed || seg_changed {
                let dwell_ns = time_ns.saturating_sub(self.latch_stable_since_ns);
                if let Some(previous) = self.digit_transition.take() {
                    let opposite_changed = if previous.com_changed {
                        seg_changed
                    } else {
                        com_changed
                    };
                    if opposite_changed && dwell_ns < previous.dwell_ns {
                        // A shorter transition must not displace the preceding
                        // real dwell, even if it passed the unpaired filter.
                        let mut restored = false;
                        for digit in 0..8 {
                            if self.com_latch & (1 << digit) == 0 {
                                continue;
                            }
                            if self.digits[digit] != previous.digits[digit] {
                                self.last_digits_change_ns = time_ns;
                                restored = true;
                            }
                            if self.digit_refresh_ns[digit] != previous.refresh_ns[digit] {
                                restored = true;
                            }
                            self.digits[digit] = previous.digits[digit];
                            self.digit_refresh_ns[digit] = previous.refresh_ns[digit];
                        }
                        if restored {
                            self.next_digit_expire_ns = 0;
                        }
                    }
                }
                self.digit_transition = Some(DigitTransition {
                    digits: self.digits,
                    refresh_ns: self.digit_refresh_ns,
                    dwell_ns: if com_changed != seg_changed
                        && self.com_latch != 0
                        && self.segment_latch != 0xFF
                    {
                        dwell_ns
                    } else {
                        0
                    },
                    com_changed,
                });
                self.latch_stable_since_ns = time_ns;
            }
            self.com_latch = latches[2];
            self.segment_latch = latches[3];
            self.last_com_strobe = versions[2];
            self.last_seg_strobe = versions[3];

            // Preserve short complete scan pulses in either latch order. A
            // leading all-off SEG is blanking, not the next digit's final data.
            let paired = if com_strobed && seg_strobed {
                true
            } else if com_strobed {
                let paired = self.pending_seg_strobe && self.segment_latch != 0xFF;
                self.pending_com_strobe = self.com_latch != 0;
                self.pending_seg_strobe = false;
                paired
            } else {
                let paired = self.pending_com_strobe;
                self.pending_seg_strobe = true;
                paired
            };
            // An unpaired all-off SEG ends illumination; it must not erase the
            // retained digit summary during inter-scan blanking.
            self.pending_digit_refresh_ns = (self.com_latch != 0
                && (self.segment_latch != 0xFF || (paired && seg_strobed) || com_strobed))
                .then_some(time_ns);
            // A SEG-first blank is a real COM slot. Confirm it (and other
            // unpaired SEG changes) after half the observed COM dwell, rather
            // than publishing a microsecond inter-latch ghost and undoing it.
            self.pending_digit_stable_ns = if self.segment_latch == 0xFF || (!paired && seg_strobed)
            {
                self.com_dwell_ns / 2
            } else {
                0
            };
            if paired
                || time_ns.saturating_sub(self.latch_stable_since_ns)
                    >= self.pending_digit_stable_ns.max(DIGIT_STABLE_NS)
            {
                self.sample_digits();
                if paired {
                    self.pending_com_strobe = false;
                    self.pending_seg_strobe = false;
                }
            }
        }

        if time_ns < self.next_digit_expire_ns {
            return;
        }
        self.next_digit_expire_ns = u64::MAX;
        for digit in 0..8 {
            if !self.digits[digit].seen {
                continue;
            }
            let elapsed_ns = time_ns.saturating_sub(self.digit_refresh_ns[digit]);
            if elapsed_ns >= DIGIT_REFRESH_TIMEOUT_NS {
                self.digits[digit] = DigitSample::default();
                self.last_digits_change_ns = time_ns;
            } else {
                self.next_digit_expire_ns = self
                    .next_digit_expire_ns
                    .min(self.digit_refresh_ns[digit].saturating_add(DIGIT_REFRESH_TIMEOUT_NS));
            }
        }
    }

    fn sample_digits(&mut self) {
        let Some(refresh_ns) = self.pending_digit_refresh_ns.take() else {
            return;
        };
        self.next_digit_expire_ns = self
            .next_digit_expire_ns
            .min(refresh_ns.saturating_add(DIGIT_REFRESH_TIMEOUT_NS));
        for digit in 0..8 {
            if self.com_latch & (1 << digit) != 0 {
                let prev = self.digits[digit];
                let next = DigitSample {
                    segments: self.segment_latch,
                    seen: true,
                };
                self.digits[digit] = next;
                // Keep the strobe timestamp: settling must not extend expiry.
                self.digit_refresh_ns[digit] = refresh_ns;
                if prev != next {
                    self.last_digits_change_ns = refresh_ns;
                }
            }
        }
    }

    pub(crate) fn display_text(&self, decoder: &SegmentDecoder) -> String {
        let mut text = String::with_capacity(16);
        for digit in self.digits {
            decoder.push_decoded_text(digit, &mut text);
        }
        text.trim_end_matches(' ').to_string()
    }

    pub(crate) fn display_text_in_range(
        &self,
        decoder: &SegmentDecoder,
        start: usize,
        end: usize,
    ) -> Result<String> {
        if start == 0 || end == 0 {
            bail!("数码管编号必须在 1..=8");
        }
        if start > end {
            bail!("数码管范围必须满足 start <= end: start={start}, end={end}");
        }
        if end > self.digits.len() {
            bail!("数码管编号必须在 1..=8");
        }

        let mut text = String::with_capacity((end - start + 1) * 2);
        for digit in &self.digits[start - 1..end] {
            decoder.push_decoded_text(*digit, &mut text);
        }
        Ok(text.trim_end_matches(' ').to_string())
    }

    pub(crate) fn seg_raw(&self, index: usize) -> Result<u8> {
        let Some(digit) = self.digits.get(index.saturating_sub(1)) else {
            bail!("数码管编号必须在 1..=8");
        };
        if !digit.seen {
            bail!("数码管 D{index} 尚未采样到有效段码");
        }
        Ok(digit.segments)
    }

    pub(crate) fn seg_pattern(&self, index: usize) -> Result<u8> {
        Ok(!self.seg_raw(index)?)
    }

    pub(crate) fn last_digits_change_ns(&self) -> u64 {
        self.last_digits_change_ns
    }
}

#[derive(Debug)]
struct DigitTransition {
    digits: [DigitSample; 8],
    refresh_ns: [u64; 8],
    dwell_ns: u64,
    com_changed: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DigitSample {
    pub(crate) segments: u8,
    pub(crate) seen: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SegmentDecoder {
    char_map: BTreeMap<u8, char>,
    blank_patterns: BTreeSet<u8>,
}

impl Default for SegmentDecoder {
    fn default() -> Self {
        let mut decoder = Self {
            char_map: BTreeMap::new(),
            blank_patterns: BTreeSet::from([0x00]),
        };
        for (pattern, ch) in [
            (0x3F, '0'),
            (0x06, '1'),
            (0x5B, '2'),
            (0x4F, '3'),
            (0x66, '4'),
            (0x6D, '5'),
            (0x7D, '6'),
            (0x07, '7'),
            (0x7F, '8'),
            (0x6F, '9'),
            (0x40, '-'),
            (0x73, 'P'),
            (0x79, 'E'),
            (0x38, 'L'),
            (0x71, 'F'),
            (0x76, 'H'),
            (0x39, 'C'),
            (0x77, 'A'),
            (0x37, 'N'),
            (0x3E, 'U'),
            (0x54, 'n'),
            (0x01, '='),
            (0x08, '_'),
        ] {
            decoder.char_map.insert(pattern, ch);
        }
        decoder
    }
}

impl SegmentDecoder {
    pub(crate) fn set_mapping(&mut self, pattern: u8, text: &str) -> Result<()> {
        let mut chars = text.chars();
        match (chars.next(), chars.next()) {
            (None, None) => {
                self.char_map.remove(&pattern);
                self.blank_patterns.insert(pattern);
                Ok(())
            }
            (Some(ch), None) => {
                self.blank_patterns.remove(&pattern);
                self.char_map.insert(pattern, ch);
                Ok(())
            }
            _ => bail!("set_seg_decode 只接受空串或单个字符"),
        }
    }

    pub(crate) fn mark_blank(&mut self, pattern: u8) {
        self.char_map.remove(&pattern);
        self.blank_patterns.insert(pattern);
    }

    pub(crate) fn decode_text(&self, digit: DigitSample) -> String {
        let mut text = String::with_capacity(2);
        self.push_decoded_text(digit, &mut text);
        text
    }

    pub(crate) fn push_decoded_text(&self, digit: DigitSample, out: &mut String) {
        if !digit.seen {
            out.push(' ');
            return;
        }
        let pattern = !digit.segments;
        if self.blank_patterns.contains(&pattern) {
            out.push(' ');
            return;
        }

        let base_pattern = pattern & 0x7F;
        let has_dot = pattern & 0x80 != 0;

        if let Some(ch) = self.char_map.get(&base_pattern).copied() {
            out.push(ch);
            if has_dot {
                out.push('.');
            }
            return;
        }

        if self.blank_patterns.contains(&base_pattern) {
            if has_dot {
                out.push('.');
            } else {
                out.push(' ');
            }
            return;
        }

        out.push('?');
        if has_dot {
            out.push('.');
        }
    }

    pub(crate) fn decode_char(&self, digit: DigitSample) -> char {
        if !digit.seen {
            return ' ';
        }
        let pattern = !digit.segments;
        if self.blank_patterns.contains(&pattern) {
            return ' ';
        }
        let base_pattern = pattern & 0x7F;
        self.char_map
            .get(&base_pattern)
            .copied()
            .unwrap_or_else(|| {
                if self.blank_patterns.contains(&base_pattern) && pattern & 0x80 == 0 {
                    ' '
                } else if pattern & 0x80 != 0 {
                    '.'
                } else {
                    '?'
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{DigitSample, Outputs, SegmentDecoder};

    #[test]
    fn display_text_keeps_decimal_point_after_digit() {
        let mut outputs = Outputs::default();
        outputs.digits[0] = DigitSample {
            segments: !0x3F,
            seen: true,
        };
        outputs.digits[1] = DigitSample {
            segments: !(0x06 | 0x80),
            seen: true,
        };
        outputs.digits[2] = DigitSample {
            segments: !0x5B,
            seen: true,
        };

        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "01.2");
    }

    #[test]
    fn decode_text_uses_dot_for_dot_only_pattern() {
        let decoder = SegmentDecoder::default();
        let digit = DigitSample {
            segments: !0x80,
            seen: true,
        };

        assert_eq!(decoder.decode_text(digit), ".");
    }

    #[test]
    fn display_text_in_range_uses_physical_digit_range() {
        let mut outputs = Outputs::default();
        outputs.digits[0] = DigitSample {
            segments: !0x3F,
            seen: true,
        };
        outputs.digits[1] = DigitSample {
            segments: !(0x06 | 0x80),
            seen: true,
        };
        outputs.digits[2] = DigitSample {
            segments: !0x5B,
            seen: true,
        };

        assert_eq!(
            outputs
                .display_text_in_range(&SegmentDecoder::default(), 2, 3)
                .expect("display range"),
            "1.2"
        );
    }

    #[test]
    fn sample_from_latches_tracks_last_effective_digit_change_time() {
        let mut outputs = Outputs::default();

        outputs.sample_from_latches(&[0x00, 0x00, 0x01, !0x3F], &[0, 0, 1, 1], 100);
        assert_eq!(outputs.last_digits_change_ns(), 100);

        outputs.sample_from_latches(&[0x00, 0x00, 0x01, !0x3F], &[0, 0, 2, 2], 200);
        assert_eq!(outputs.last_digits_change_ns(), 100);

        outputs.sample_from_latches(&[0x00, 0x00, 0x01, !0x06], &[0, 0, 3, 3], 300);
        assert_eq!(outputs.last_digits_change_ns(), 300);
    }

    #[test]
    fn digit_without_refresh_blanks_after_timeout() {
        let mut outputs = Outputs::default();
        outputs.sample_from_latches(&[0x00, 0x00, 0x01, !0x3F], &[0, 0, 1, 1], 100);
        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "0");

        outputs.sample_from_latches(&[0x00, 0x00, 0x01, !0x3F], &[0, 0, 1, 1], 100_000_099);
        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "0");

        outputs.sample_from_latches(&[0x00, 0x00, 0x01, !0x3F], &[0, 0, 1, 1], 100_000_100);
        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "");
    }

    #[test]
    fn expiry_scan_tracks_staggered_digits_and_new_refresh_after_blank() {
        let mut outputs = Outputs::default();
        outputs.sample_from_latches(&[0, 0, 1, !0x3F], &[0, 0, 1, 1], 100);
        outputs.sample_from_latches(&[0, 0, 2, !0x06], &[0, 0, 2, 2], 200);
        outputs.sample_from_latches(&[0, 0, 2, !0x06], &[0, 0, 2, 2], 100_000_099);
        assert!(outputs.digits[0].seen && outputs.digits[1].seen);
        outputs.sample_from_latches(&[0, 0, 2, !0x06], &[0, 0, 2, 2], 100_000_100);
        assert!(!outputs.digits[0].seen && outputs.digits[1].seen);
        outputs.sample_from_latches(&[0, 0, 2, !0x06], &[0, 0, 2, 2], 100_000_200);
        assert!(!outputs.digits[1].seen);

        outputs.sample_from_latches(&[0, 0, 1, !0x3F], &[0, 0, 3, 3], 200_000_000);
        outputs.sample_from_latches(&[0, 0, 1, !0x3F], &[0, 0, 3, 3], 299_999_999);
        assert!(outputs.digits[0].seen);
        outputs.sample_from_latches(&[0, 0, 1, !0x3F], &[0, 0, 3, 3], 300_000_000);
        assert!(!outputs.digits[0].seen);
    }

    #[test]
    fn repeated_scan_of_same_pattern_keeps_digit_visible() {
        let mut outputs = Outputs::default();
        outputs.sample_from_latches(&[0x00, 0x00, 0x01, !0x3F], &[0, 0, 1, 1], 100);
        outputs.sample_from_latches(&[0x00, 0x00, 0x01, !0x3F], &[0, 0, 2, 2], 90_000_100);
        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "0");

        outputs.sample_from_latches(&[0x00, 0x00, 0x01, !0x3F], &[0, 0, 2, 2], 180_000_099);
        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "0");

        outputs.sample_from_latches(&[0x00, 0x00, 0x01, !0x3F], &[0, 0, 2, 2], 190_000_100);
        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "");
    }

    #[test]
    fn stable_latch_outputs_support_both_scan_orders_without_shift() {
        // 74HC_HCT573.pdf p1, p6 Table 4: both Q banks retain their outputs.
        // SCH_V31.pdf p3: U8 drives COM1..8; U7 drives active-low A..DP.
        for (segment_first, gap_ns) in [(false, 100), (true, 100), (false, 10_000), (true, 10_000)]
        {
            let mut outputs = Outputs::default();
            let mut latches = [0, 0, 0, 0xFF];
            let mut versions = [0; 4];
            let patterns = [0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07];
            for frame in 0..2 {
                for (digit, pattern) in patterns.into_iter().enumerate() {
                    let start = 1_000_000 * (1 + frame * 8 + digit as u64);
                    for (offset, slot) in if segment_first { [3, 2] } else { [2, 3] }
                        .into_iter()
                        .enumerate()
                    {
                        latches[slot] = if slot == 2 { 1 << digit } else { !pattern };
                        versions[slot] += 1;
                        outputs.sample_from_latches(
                            &latches,
                            &versions,
                            start + offset as u64 * gap_ns,
                        );
                    }
                    outputs.sample_from_latches(&latches, &versions, start + 500_000);
                    for scanned in 0..=digit {
                        assert_eq!(
                            outputs.seg_pattern(scanned + 1).unwrap_or_else(|error| {
                                panic!("segment_first={segment_first}, gap_ns={gap_ns}, frame={frame}, digit={scanned}: {error}")
                            }),
                            patterns[scanned],
                            "segment_first={segment_first}, gap_ns={gap_ns}, frame={frame}, digit={scanned}"
                        );
                        assert_eq!(
                            outputs.digit_refresh_ns[scanned],
                            1_000_000 * (1 + frame * 8 + scanned as u64) + gap_ns,
                            "a transition must not extend the preceding digit's lifetime"
                        );
                    }
                }
            }
            assert_eq!(outputs.display_text(&SegmentDecoder::default()), "01234567");
            let first_expiry = 109_000_000 + gap_ns;
            outputs.sample_from_latches(&latches, &versions, first_expiry - 1);
            assert!(outputs.digits.iter().all(|digit| digit.seen));
            outputs.sample_from_latches(&latches, &versions, first_expiry);
            assert!(!outputs.digits[0].seen);
            assert!(outputs.digits[1..].iter().all(|digit| digit.seen));
            assert_eq!(outputs.next_digit_expire_ns, first_expiry + 1_000_000);
            outputs.sample_from_latches(&latches, &versions, first_expiry + 1);
            assert!(!outputs.digits[0].seen);
        }
    }

    #[test]
    fn stable_latch_outputs_ignore_short_blank_and_ghost_before_real_dwell() {
        let mut outputs = Outputs::default();
        outputs.sample_from_latches(&[0, 0, 1, !0x3F], &[0, 0, 1, 1], 100);
        outputs.sample_from_latches(&[0, 0, 1, !0x3F], &[0, 0, 1, 1], 1_000_000);

        // A short SEG-first transition must not replace the previous digit.
        outputs.sample_from_latches(&[0, 0, 1, !0x06], &[0, 0, 1, 2], 1_000_100);
        outputs.sample_from_latches(&[0, 0, 2, !0x06], &[0, 0, 2, 2], 1_000_200);
        outputs.sample_from_latches(&[0, 0, 2, !0x06], &[0, 0, 2, 2], 2_000_000);
        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "01");

        // Blanking and a short wrong segment precede the same real dwell.
        outputs.sample_from_latches(&[0, 0, 2, 0xFF], &[0, 0, 2, 3], 2_000_100);
        outputs.sample_from_latches(&[0, 0, 2, !0x7F], &[0, 0, 2, 4], 2_000_200);
        outputs.sample_from_latches(&[0, 0, 1, !0x7F], &[0, 0, 3, 4], 2_000_300);
        outputs.sample_from_latches(&[0, 0, 1, !0x3F], &[0, 0, 3, 5], 2_000_400);
        outputs.sample_from_latches(&[0, 0, 1, !0x3F], &[0, 0, 3, 5], 3_000_000);
        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "01");
        assert!(
            outputs.digits[0].seen,
            "a real dwell must retain the valid sample"
        );
    }

    #[test]
    fn stable_latch_outputs_handle_com_off_and_segment_only_updates() {
        let mut outputs = Outputs::default();
        outputs.sample_from_latches(&[0, 0, 0, !0x3F], &[0, 0, 0, 1], 100);
        outputs.sample_from_latches(&[0, 0, 0, !0x3F], &[0, 0, 0, 1], 1_000_000);
        assert!(outputs.digits.iter().all(|digit| !digit.seen));

        outputs.sample_from_latches(&[0, 0, 3, !0x3F], &[0, 0, 1, 1], 1_000_100);
        outputs.sample_from_latches(&[0, 0, 3, !0x3F], &[0, 0, 1, 1], 2_000_000);
        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "00");

        outputs.sample_from_latches(&[0, 0, 3, !0x06], &[0, 0, 1, 2], 2_000_100);
        outputs.sample_from_latches(&[0, 0, 3, !0x06], &[0, 0, 1, 2], 3_000_000);
        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "11");

        outputs.sample_from_latches(&[0, 0, 3, 0xFF], &[0, 0, 1, 3], 3_000_100);
        outputs.sample_from_latches(&[0, 0, 3, 0xFF], &[0, 0, 1, 3], 4_000_000);
        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "11");
        outputs.sample_from_latches(&[0, 0, 3, 0xFF], &[0, 0, 1, 3], 102_000_100);
        assert!(outputs.digits.iter().all(|digit| !digit.seen));

        // An explicit COM + blank SEG refresh still represents a blank digit.
        outputs.sample_from_latches(&[0, 0, 3, 0xFF], &[0, 0, 2, 4], 103_000_000);
        assert_eq!(outputs.seg_raw(1).expect("stable blank D1"), 0xFF);
        assert_eq!(outputs.seg_raw(2).expect("stable blank D2"), 0xFF);
        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "");
    }

    #[test]
    fn stable_latch_outputs_keep_exact_staggered_expiry_after_settling() {
        for segment_first in [false, true] {
            let mut outputs = Outputs::default();
            let mut latches = [0, 0, 0, 0xFF];
            let mut versions = [0; 4];
            for (digit, pattern) in [0x3F, 0x06].into_iter().enumerate() {
                let start = 100 + digit as u64 * 10_000;
                for (offset, slot) in if segment_first { [3, 2] } else { [2, 3] }
                    .into_iter()
                    .enumerate()
                {
                    latches[slot] = if slot == 2 { 1 << digit } else { !pattern };
                    versions[slot] += 1;
                    outputs.sample_from_latches(&latches, &versions, start + offset as u64 * 100);
                }
                outputs.sample_from_latches(&latches, &versions, start + 2_000);
            }
            assert_eq!(outputs.display_text(&SegmentDecoder::default()), "01");
            assert_eq!(outputs.next_digit_expire_ns, 100_000_200);
            for time_ns in [100_000_199, 100_000_200, 100_000_201] {
                outputs.sample_from_latches(&latches, &versions, time_ns);
                assert_eq!(outputs.digits[0].seen, time_ns < 100_000_200);
                assert!(outputs.digits[1].seen);
            }
            assert_eq!(outputs.next_digit_expire_ns, 100_010_200);
            for time_ns in [100_010_199, 100_010_200, 100_010_201] {
                outputs.sample_from_latches(&latches, &versions, time_ns);
                assert_eq!(outputs.digits[1].seen, time_ns < 100_010_200);
            }
            assert_eq!(outputs.next_digit_expire_ns, u64::MAX);

            // A new COM-only strobe after expiry must restart the P deadline.
            versions[2] += 1;
            outputs.sample_from_latches(&latches, &versions, 200_000_000);
            assert_eq!(outputs.seg_pattern(2).expect("refreshed D2"), 0x06);
            assert_eq!(outputs.next_digit_expire_ns, 300_000_000);
        }
    }

    #[test]
    fn stable_latch_outputs_dwell_boundary_does_not_extend_refresh_time() {
        // The 1us boundary is the digit-summary policy, not a chip timing limit.
        for dwell_ns in [999, 1_000, 1_001] {
            let mut outputs = Outputs::default();
            outputs.sample_from_latches(&[0, 0, 1, !0x3F], &[0, 0, 1, 1], 100);
            outputs.sample_from_latches(&[0, 0, 1, !0x06], &[0, 0, 1, 2], 10_000);
            outputs.sample_from_latches(&[0, 0, 1, !0x06], &[0, 0, 1, 2], 10_000 + dwell_ns);
            assert_eq!(
                outputs.seg_pattern(1).expect("D1"),
                if dwell_ns < 1_000 { 0x3F } else { 0x06 }
            );
            outputs.sample_from_latches(&[0, 0, 0, !0x06], &[0, 0, 2, 2], 20_000);
            outputs.sample_from_latches(&[0, 0, 0, !0x06], &[0, 0, 2, 2], 100_009_999);
            assert!(outputs.digits[0].seen);
            outputs.sample_from_latches(&[0, 0, 0, !0x06], &[0, 0, 2, 2], 100_010_000);
            assert!(!outputs.digits[0].seen);
        }
    }

    #[test]
    fn stable_latch_outputs_same_value_strobes_do_not_restart_dwell() {
        let mut outputs = Outputs::default();
        outputs.sample_from_latches(&[0, 0, 1, !0x3F], &[0, 0, 1, 1], 100);
        outputs.sample_from_latches(&[0, 0, 1, !0x06], &[0, 0, 1, 2], 10_000);
        outputs.sample_from_latches(&[0, 0, 1, !0x06], &[0, 0, 1, 3], 10_500);
        outputs.sample_from_latches(&[0, 0, 1, !0x06], &[0, 0, 1, 4], 10_999);
        assert_eq!(outputs.seg_pattern(1).expect("D1 before settling"), 0x3F);
        outputs.sample_from_latches(&[0, 0, 1, !0x06], &[0, 0, 1, 4], 11_000);
        assert_eq!(outputs.seg_pattern(1).expect("D1 after settling"), 0x06);
        outputs.sample_from_latches(&[0, 0, 1, !0x06], &[0, 0, 1, 4], 100_010_998);
        assert!(outputs.digits[0].seen);
        outputs.sample_from_latches(&[0, 0, 1, !0x06], &[0, 0, 1, 4], 100_010_999);
        assert!(!outputs.digits[0].seen);
    }

    #[test]
    fn stable_latch_outputs_keep_short_paired_scan_pulses() {
        for transitions in [
            vec![
                (0, 0xFF, 0, 1),
                (1, 0xFF, 1, 1),
                (1, !0x3F, 1, 2),
                (1, 0xFF, 1, 3),
                (2, 0xFF, 2, 3),
                (2, !0x06, 2, 4),
                (2, 0xFF, 2, 5),
                (0, 0xFF, 3, 5),
            ],
            vec![
                (0, !0x3F, 0, 1),
                (1, !0x3F, 1, 1),
                (0, !0x3F, 2, 1),
                (0, !0x06, 2, 2),
                (2, !0x06, 3, 2),
                (0, !0x06, 4, 2),
            ],
        ] {
            let mut outputs = Outputs::default();
            for (index, (com, segments, com_version, seg_version)) in
                transitions.into_iter().enumerate()
            {
                outputs.sample_from_latches(
                    &[0, 0, com, segments],
                    &[0, 0, com_version, seg_version],
                    100 * (index as u64 + 1),
                );
            }
            assert_eq!(outputs.display_text(&SegmentDecoder::default()), "01");
        }
    }

    #[test]
    fn stable_latch_outputs_keep_com_only_dwells_of_different_lengths() {
        let mut outputs = Outputs::default();
        outputs.sample_from_latches(&[0, 0, 1, !0x3F], &[0, 0, 1, 1], 100);
        outputs.sample_from_latches(&[0, 0, 2, !0x3F], &[0, 0, 2, 1], 2_000_100);
        outputs.sample_from_latches(&[0, 0, 2, !0x3F], &[0, 0, 2, 1], 2_002_100);
        outputs.sample_from_latches(&[0, 0, 4, !0x3F], &[0, 0, 3, 1], 3_000_100);
        outputs.sample_from_latches(&[0, 0, 4, !0x3F], &[0, 0, 3, 1], 3_002_100);
        assert_eq!(outputs.display_text(&SegmentDecoder::default()), "000");
        assert_eq!(outputs.digit_refresh_ns[1], 2_000_100);
    }

    #[test]
    fn mixed_blank_seg_first_refresh_keeps_effective_change_time_stable() {
        // Continuous 0xFF digits are real scan slots, not inter-scan blanking.
        // SCH_V31.pdf p3 and 74HC_HCT573.pdf p1/p6: SEG and COM outputs coexist.
        let segments = [0xFF, 0xFF, 0xC0, 0xF9, 0xFF, 0xA4, 0xFF, 0xFF];
        let expected_digits = segments.map(|segments| DigitSample {
            segments,
            seen: true,
        });
        for gap_ns in [2_000, 10_000] {
            let mut outputs = Outputs::default();
            let mut latches = [0, 0, 0, 0xFF];
            let mut versions = [0; 4];
            let mut stable_change_ns = 0;
            for frame in 0..6 {
                for (digit, segment) in segments.into_iter().enumerate() {
                    let start = (1 + frame * 8 + digit as u64) * 1_000_000;
                    latches[3] = segment;
                    versions[3] += 1;
                    for offset in (0..gap_ns).step_by(500) {
                        outputs.sample_from_latches(&latches, &versions, start + offset);
                        if frame >= 2 {
                            assert_eq!(
                                outputs.last_digits_change_ns(),
                                stable_change_ns,
                                "refresh changed the stable clock: gap={gap_ns}, digit={digit}, offset={offset}"
                            );
                            assert_eq!(outputs.digits, expected_digits);
                        }
                    }
                    latches[2] = 1 << digit;
                    versions[2] += 1;
                    for offset in [gap_ns, gap_ns + 2_000, 750_000] {
                        outputs.sample_from_latches(&latches, &versions, start + offset);
                        if frame >= 2 {
                            assert_eq!(outputs.last_digits_change_ns(), stable_change_ns);
                            assert_eq!(outputs.digits, expected_digits);
                        }
                    }
                }
                if frame == 1 {
                    stable_change_ns = outputs.last_digits_change_ns();
                }
            }
            assert_eq!(outputs.digits.map(|digit| digit.segments), segments);
            assert!(outputs.digits.iter().all(|digit| digit.seen));
            assert_eq!(outputs.display_text(&SegmentDecoder::default()), "  01 2");
        }
    }

    fn mixed_blank_scan_firmware(change: Option<u8>) -> Vec<u8> {
        // 74HC_HCT573.pdf p1/p6 Table 4: LE high makes Q follow shared P0.
        // docs/G1-report.md G1R §3: P0=COM overwrote SEG, holding wrong
        // (01,01)/(02,02)/(04,04) pairs for 670250ns. Close both latches
        // before each P0 change, preserving SEG-first order and real dwell.
        let mut code = vec![0x75, 0x30, 0xC0]; // MOV 30H,#0C0H (digit 0)
        if change.is_some() {
            code.extend_from_slice(&[0x7F, 4]); // MOV R7,#4 (change after four frames)
        }
        let frame_start = code.len() as u16;
        for (digit, segments) in [0xFF, 0xFF, 0xC0, 0xF9, 0xFF, 0xA4, 0xFF, 0xFF]
            .into_iter()
            .enumerate()
        {
            code.extend_from_slice(&[0x75, 0xA0, 0x00]); // MOV P2,#00H (deselect)
            if digit == 2 {
                code.extend_from_slice(&[0x85, 0x30, 0x80]); // MOV P0,30H
            } else {
                code.extend_from_slice(&[0x75, 0x80, segments]); // MOV P0,#segments
            }
            code.extend_from_slice(&[
                0x75, 0xA0, 0xE0, // MOV P2,#0E0H (SEG first)
                0x75, 0xA0, 0x00, // MOV P2,#00H (hold SEG before P0=COM)
                0x7E, 12, // MOV R6,#12
                0xDE, 0xFE, // DJNZ R6,$ (microsecond SEG/COM gap)
            ]);
            code.extend_from_slice(&[0x75, 0x80, 1 << digit]); // MOV P0,#COM
            code.extend_from_slice(&[
                0x75, 0xA0, 0xC0, // MOV P2,#0C0H
                0x7C, 8, // MOV R4,#8
                0x7D, 250, // MOV R5,#250
                0xDD, 0xFE, // DJNZ R5,$
                0xDC, 0xFA, // DJNZ R4,outer (sub-millisecond real dwell)
            ]);
        }
        if let Some(segments) = change {
            code.extend_from_slice(&[
                0xDF, 3, // DJNZ R7,skip_change
                0x75, 0x30, segments, // MOV 30H,#segments
            ]);
        }
        code.extend_from_slice(&[0x02, (frame_start >> 8) as u8, frame_start as u8]); // LJMP frame
        code
    }

    #[test]
    fn mixed_blank_seg_first_firmware_observation_window_returns_and_extends() {
        use crate::{Simulator, wave::WaveCaptureOptions};

        let mut sim = Simulator::from_code_with_options(
            mixed_blank_scan_firmware(None),
            false,
            WaveCaptureOptions::default(),
        );
        sim.run_ms(20).expect("warm up mixed blank scan");
        // Fail in bounded instruction stepping before calling an observer that
        // would otherwise loop forever on the broken continuously changing scan.
        let check_until = sim.sim_time_ns() + 10_000_000;
        while sim.sim_time_ns() < check_until {
            sim.step_instruction().expect("step mixed blank scan");
            assert_eq!(sim.display_text(), "  01 2", "refresh exposed a scan ghost");
        }
        for (digit, segments) in [0xFF, 0xFF, 0xC0, 0xF9, 0xFF, 0xA4, 0xFF, 0xFF]
            .into_iter()
            .enumerate()
        {
            assert_eq!(sim.seg_raw(digit + 1).expect("sampled digit"), segments);
        }
        let start = sim.sim_time_ns();
        assert_eq!(
            sim.observe_display_text(30).expect("stable window"),
            "  01 2"
        );
        assert!((30_000_000..31_000_000).contains(&(sim.sim_time_ns() - start)));

        // Real numeric and real blank changes during the window must both extend it.
        for (segments, expected) in [(0xA4, "  21 2"), (0xFF, "   1 2")] {
            let mut sim = Simulator::from_code_with_options(
                mixed_blank_scan_firmware(Some(segments)),
                false,
                WaveCaptureOptions::default(),
            );
            sim.run_ms(12).expect("warm up before the fourth frame");
            assert_eq!(sim.display_text(), "  01 2");
            let start = sim.sim_time_ns();
            assert_eq!(
                sim.observe_display_text(30).expect("changed window"),
                expected
            );
            assert!((31_000_000..50_000_000).contains(&(sim.sim_time_ns() - start)));
        }
    }

    #[test]
    fn paired_com_first_blank_still_samples_immediately() {
        let mut outputs = Outputs::default();
        outputs.sample_from_latches(&[0, 0, 1, 0xC0], &[0, 0, 1, 1], 100);
        outputs.sample_from_latches(&[0, 0, 1, 0xC0], &[0, 0, 2, 1], 2_000_000);
        outputs.sample_from_latches(&[0, 0, 1, 0xFF], &[0, 0, 2, 2], 2_000_100);
        assert_eq!(outputs.seg_raw(1).expect("explicit blank"), 0xFF);
        assert_eq!(outputs.last_digits_change_ns(), 2_000_100);
    }
}
