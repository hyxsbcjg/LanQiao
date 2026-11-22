pub mod bench;
pub mod build_info;
mod chip;
mod event;
mod hex;
mod ids;
mod jumper;
mod peripherals;
mod persistent_state;
mod wave;

pub use bench::BenchHarness;
pub use chip::run_target::{RunToEdge, RunToTarget};
pub use chip::{
    BoardSnapshot, BoundedRunStats, CPU_EXEC_HZ, DisplayNumber, NS_PER_MICROSECOND,
    NS_PER_MILLISECOND, NS_PER_SECOND, SYSTEM_HZ, Simulator, UartConfig, UartParity, UartStopBits,
};
pub use ids::{KeyId, KeyMode, LedId, ResetMode, SignalId, VoltageChannel};
pub use peripherals::{Brightness, Ds1302State, RelayEvent};
pub use wave::WaveCaptureOptions;
