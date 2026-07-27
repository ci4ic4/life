//! wgpu compute + render harness.
//!
//! `sim` and `evolve_sim` compute on the GPU. `ecology_sim` does not — it steps on
//! the CPU and owns the texture the renderer reads, because its RNG contract rules
//! out a parallel step. What the three share is that shape: a texture view the
//! renderer can paint from, so `life-app` treats them alike.
pub mod context;
pub mod ecology_sim;
pub mod evolve_sim;
pub mod sim;
pub use context::GpuContext;
pub use ecology_sim::EcologySim;
pub use evolve_sim::{evolve_self_test, EvolveSim};
pub use sim::{gpu_self_test, Sim, SimRule};
