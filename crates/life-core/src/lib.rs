//! Pure Life simulation math. No GPU, no window.
pub mod detect;
pub mod grid;
pub mod rule;
pub mod topology;

pub use detect::CycleDetector;
pub use grid::{count_neighbors, step_deterministic, Grid};
pub use rule::{parse_bs, RuleErr, BS};
pub use topology::{resolve, Topology, Wrap};
