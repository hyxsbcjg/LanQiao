//! PCA 公共 16 位计数器；手册 pp.784–785、791–792。
use super::super::registers::*;
use super::{read_sfr, write_sfr};

#[derive(Debug, Default)]
pub(super) struct Pca {
    divider: u64,
}

impl Pca {
    fn running(generic: &[u8; 128]) -> bool {
        read_sfr(generic, SFR_CCON) & CCON_CR != 0
            && !(read_sfr(generic, SFR_PCON) & PCON_IDL != 0
                && read_sfr(generic, SFR_CMOD) & CMOD_CIDL != 0)
    }

    pub(super) fn tick(&mut self, ticks: u32, generic: &mut [u8; 128]) {
        if read_sfr(generic, SFR_CCON) & CCON_CR == 0 {
            self.divider = 0;
            return;
        }
        if !Self::running(generic) {
            return;
        }
        let source = (read_sfr(generic, SFR_CMOD) >> 1) & 7;
        let divisor = match source {
            0 => 12,
            1 => 2,
            4 => 1,
            5 => 4,
            6 => 6,
            7 => 8,
            _ => return,
        };
        self.divider += u64::from(ticks);
        let increments = self.divider / divisor;
        self.divider %= divisor;
        Self::increment(increments, generic);
    }

    pub(super) fn timer0_overflows(&mut self, overflows: u32, generic: &mut [u8; 128]) {
        if (read_sfr(generic, SFR_CMOD) >> 1) & 7 == 2 && Self::running(generic) {
            Self::increment(u64::from(overflows), generic);
        }
    }

    pub(super) fn sample_eci(&mut self, previous_p1: u8, next_p1: u8, generic: &mut [u8; 128]) {
        if (read_sfr(generic, SFR_CMOD) >> 1) & 7 == 3
            && previous_p1 & !next_p1 & (1 << 2) != 0
            && Self::running(generic)
        {
            Self::increment(1, generic);
        }
    }

    fn increment(increments: u64, generic: &mut [u8; 128]) {
        let counter = u16::from_be_bytes([read_sfr(generic, SFR_CH), read_sfr(generic, SFR_CL)]);
        let total = u64::from(counter) + increments;
        let [ch, cl] = (total as u16).to_be_bytes();
        write_sfr(generic, SFR_CH, ch);
        write_sfr(generic, SFR_CL, cl);
        if total > u64::from(u16::MAX) {
            write_sfr(generic, SFR_CCON, read_sfr(generic, SFR_CCON) | CCON_CF);
        }
    }
}
