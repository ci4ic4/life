//! wgpu compute + render harness.
pub mod context;
pub mod sim;
pub use context::GpuContext;
pub use sim::{gpu_self_test, Sim, SimRule};
