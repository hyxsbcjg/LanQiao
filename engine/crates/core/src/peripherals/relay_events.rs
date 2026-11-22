const EVENT_CAP: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayEvent {
    pub t_ns: u64,
    pub on: bool,
}

#[derive(Debug, Default)]
pub(crate) struct RelayEvents {
    on: bool,
    events: Vec<RelayEvent>,
    dropped: u64,
}

impl RelayEvents {
    pub(crate) fn sample(&mut self, on: bool, t_ns: u64) {
        if on == self.on {
            return;
        }
        self.on = on;
        if self.events.len() < EVENT_CAP {
            self.events.push(RelayEvent { t_ns, on });
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    pub(crate) fn take(&mut self) -> (Vec<RelayEvent>, u64) {
        let events = self.events.drain(..).collect();
        (events, std::mem::take(&mut self.dropped))
    }
}

#[cfg(test)]
mod tests {
    use super::{RelayEvent, RelayEvents};

    #[test]
    fn relay_events_cap_boundaries_and_next_window_keep_raw_drive_state() {
        for count in [255, 256, 257, 300] {
            let mut buffer = RelayEvents::default();
            for index in 0..count {
                buffer.sample(index % 2 == 0, index + 1);
            }
            let (events, dropped) = buffer.take();
            assert_eq!(events.len(), count.min(256) as usize);
            assert_eq!(dropped, count.saturating_sub(256));
            for (index, event) in events.iter().enumerate() {
                assert_eq!(event.t_ns, index as u64 + 1);
                assert_eq!(event.on, index % 2 == 0);
            }
            buffer.sample(count % 2 == 1, count + 1); // Same raw state after overflow.
            assert_eq!(buffer.take(), (vec![], 0));
            buffer.sample(count % 2 == 0, count + 2);
            assert_eq!(
                buffer.take(),
                (vec![RelayEvent { t_ns: count + 2, on: count % 2 == 0 }], 0)
            );
        }
    }
}
