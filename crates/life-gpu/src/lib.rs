//! wgpu compute + render harness.
pub mod context;
pub mod evolve_sim;
pub mod sim;
pub use context::GpuContext;
pub use evolve_sim::{evolve_self_test, EvolveSim};
pub use sim::{gpu_self_test, Sim, SimRule};
