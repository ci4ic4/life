//! Pure Life simulation math. No GPU, no window.
pub mod detect;
pub mod grid;
pub mod prob;
pub mod rule;
pub mod stats;
pub mod topology;
pub mod trial;

pub use detect::CycleDetector;
pub use grid::{count_neighbors, step_deterministic, step_stochastic, Grid};
pub use prob::{bump_max, curve, CurveParams};
pub use rule::{parse_bs, RuleErr, BS};
pub use stats::{classify, RingStats, Thresholds, Verdict};
pub use topology::{resolve, Topology, Wrap};
pub use trial::{run_trial, TrialOpts, TrialResult};
