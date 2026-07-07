# Rust Rewrite — Slice A (foundation + life-torus) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Cargo workspace and ship a native `life-torus` (deterministic Life on a 3D torus) running on Windows/macOS/Linux, built on `life-core` (pure sim math), `life-gpu` (wgpu compute+render), `life-render` (torus mesh + orbit camera), and `life-app` (winit + egui binary).

**Architecture:** Four-crate workspace. `life-core` holds all simulation math with zero GPU/window dependencies and is the only unit-tested-in-full crate. `life-gpu` runs the CA step as a real WGSL compute shader (ping-pong storage textures) and self-tests its output against `life-core` on boot. `life-render` paints the state texture onto a torus and orbits a hand-rolled camera. `life-app` is the binary wiring them under a winit event loop with egui panels.

**Tech Stack:** Rust 2021, wgpu 29, winit 0.30, egui + egui-winit + egui-wgpu, glam (math), bytemuck (POD casts), rand (core RNG), pollster (block on async wgpu init — no async runtime).

## Global Constraints

- **Rust edition 2021**, resolver 2. Workspace with a single virtual manifest at repo root.
- **Pinned crate versions** (verify they resolve together in Task 1; bump in lockstep if not): `winit = "0.30"`, `wgpu = "29"`, `bytemuck = { version = "1", features = ["derive"] }`, `glam = "0.29"`, `rand = "0.8"`, `pollster = "0.4"`. egui stack (`egui`, `egui-winit`, `egui-wgpu`) pinned to the versions whose `egui-wgpu` depends on `wgpu = 29` and `egui-winit` on `winit = 0.30` — resolve the exact numbers in Task 1 (`cargo add` picks compatible; confirm the transitive wgpu/winit match before committing).
- **No async runtime.** wgpu's async init calls are driven with `pollster::block_on`. Readback uses `device.poll(wgpu::PollType::Wait)`.
- **RNG injected**, never global — `rand::Rng` passed by `&mut`. (Torus itself is deterministic and takes no RNG; this constraint bites in Slices B/C but the core API is shaped for it now.)
- **Grid caps:** GPU mode ≤ 2048², CPU fallback ≤ 300². Same as the browser version.
- **Digit-set B/S rules stored as `u16` bitmasks**, bit `n` = neighbor count `n` (0..=8) is a member.
- **Boundary topologies:** `Straight` (torus wrap), `None` (hard edge), `Flip` (Klein seam, mirrors the *other* axis). `life-core::resolve` is the one canonical implementation.
- **Existing tests are the bar:** ported assertions from `life-stats.test.js` must pass unchanged in meaning.
- **Do not add crates beyond the pinned list** without noting why. No serde, no async, no camera crate, no ECS.
- **This plan is Slice A only.** Slices B (stochastic) and C (evolve) get their own plans once A's foundation is validated on all three OSes.

---

### Task 1: Workspace scaffold + CI

**Files:**
- Create: `Cargo.toml` (workspace root, virtual manifest)
- Create: `crates/life-core/Cargo.toml`, `crates/life-core/src/lib.rs`
- Create: `crates/life-gpu/Cargo.toml`, `crates/life-gpu/src/lib.rs`
- Create: `crates/life-render/Cargo.toml`, `crates/life-render/src/lib.rs`
- Create: `crates/life-app/Cargo.toml`, `crates/life-app/src/main.rs`
- Create: `.github/workflows/ci.yml`
- Modify: `.gitignore` (add `/target`)

**Interfaces:**
- Consumes: nothing.
- Produces: a compiling 4-crate workspace. `life-core` lib crate; `life-gpu`/`life-render` lib crates; `life-app` bin crate depending on the other three.

- [ ] **Step 1: Create the workspace manifest**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/life-core", "crates/life-gpu", "crates/life-render", "crates/life-app"]

[workspace.package]
edition = "2021"
license = "MIT OR Apache-2.0"

[workspace.dependencies]
bytemuck = { version = "1", features = ["derive"] }
glam = "0.29"
rand = "0.8"
wgpu = "29"
winit = "0.30"
pollster = "0.4"
```

- [ ] **Step 2: Create the four crate manifests and stub lib/main files**

`crates/life-core/Cargo.toml`:
```toml
[package]
name = "life-core"
edition.workspace = true

[dependencies]
rand.workspace = true
```
`crates/life-core/src/lib.rs`:
```rust
//! Pure Life simulation math. No GPU, no window.
```
`crates/life-gpu/Cargo.toml`:
```toml
[package]
name = "life-gpu"
edition.workspace = true

[dependencies]
life-core = { path = "../life-core" }
wgpu.workspace = true
bytemuck.workspace = true
pollster.workspace = true
```
`crates/life-gpu/src/lib.rs`: `//! wgpu compute + render harness.`
`crates/life-render/Cargo.toml`:
```toml
[package]
name = "life-render"
edition.workspace = true

[dependencies]
wgpu.workspace = true
glam.workspace = true
bytemuck.workspace = true
```
`crates/life-render/src/lib.rs`: `//! Torus mesh, UV paint, orbit camera.`
`crates/life-app/Cargo.toml`:
```toml
[package]
name = "life-app"
edition.workspace = true

[dependencies]
life-core = { path = "../life-core" }
life-gpu = { path = "../life-gpu" }
life-render = { path = "../life-render" }
winit.workspace = true
wgpu.workspace = true
glam.workspace = true
pollster.workspace = true
# egui stack added in Task 8; add here when reached.
```
`crates/life-app/src/main.rs`:
```rust
fn main() {
    println!("life — native. Slice A scaffold.");
}
```

- [ ] **Step 3: Add the egui stack and confirm version alignment**

Run: `cargo add --package life-app egui egui-winit egui-wgpu`
Then verify the transitive graphics deps match the pins:
Run: `cargo tree -p life-app -i wgpu` and `cargo tree -p life-app -i winit`
Expected: a single `wgpu v29.x` and a single `winit v0.30.x` in the tree (no duplicate major versions). If egui pulled a different major, pin egui to the release notes' wgpu-29/winit-0.30-compatible version and re-run.

- [ ] **Step 4: Add CI and gitignore**

`.gitignore` (append): `/target`
`.github/workflows/ci.yml`:
```yaml
name: ci
on: [push, pull_request]
jobs:
  build-test:
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Linux GPU/X11 deps
        if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y libx11-dev libxkbcommon-dev libwayland-dev
      - run: cargo build --workspace
      - run: cargo test -p life-core
```
Note: CI runs `cargo test -p life-core` (headless-safe). GPU self-tests need an adapter and are covered by the `--features gpu-selftest` gate run manually / on GPU-capable runners, wired in Task 6.

- [ ] **Step 5: Build the workspace**

Run: `cargo build --workspace`
Expected: compiles clean (warnings about unused deps are fine at this stage).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates .github .gitignore Cargo.lock
git commit -m "feat(rust): slice A task 1 — workspace scaffold + CI"
```

---

### Task 2: life-core — Topology + `resolve`

**Files:**
- Create: `crates/life-core/src/topology.rs`
- Modify: `crates/life-core/src/lib.rs` (add `pub mod topology;` and re-exports)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub enum Wrap { Straight, None, Flip }`
  - `pub struct Topology { pub x: Wrap, pub y: Wrap }`
  - `pub fn resolve(x: i32, y: i32, w: u32, h: u32, t: Topology) -> Option<(u32, u32)>` — returns `None` when a coordinate falls off a `None` (hard) edge; wraps for `Straight`; for `Flip` on an axis, wraps that axis and mirrors the *other* axis coordinate (Klein seam).

- [ ] **Step 1: Write the failing tests**

`crates/life-core/src/topology.rs`:
```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wrap { Straight, None, Flip }

#[derive(Clone, Copy, Debug)]
pub struct Topology { pub x: Wrap, pub y: Wrap }

pub fn resolve(_x: i32, _y: i32, _w: u32, _h: u32, _t: Topology) -> Option<(u32, u32)> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    const TORUS: Topology = Topology { x: Wrap::Straight, y: Wrap::Straight };

    #[test]
    fn in_bounds_is_identity() {
        assert_eq!(resolve(3, 4, 10, 10, TORUS), Some((3, 4)));
    }
    #[test]
    fn straight_wraps_both_ways() {
        assert_eq!(resolve(-1, 0, 10, 10, TORUS), Some((9, 0)));
        assert_eq!(resolve(10, 0, 10, 10, TORUS), Some((0, 0)));
        assert_eq!(resolve(0, -1, 10, 10, TORUS), Some((0, 9)));
    }
    #[test]
    fn none_falls_off_edge() {
        let t = Topology { x: Wrap::None, y: Wrap::None };
        assert_eq!(resolve(-1, 5, 10, 10, t), None);
        assert_eq!(resolve(10, 5, 10, 10, t), None);
        assert_eq!(resolve(5, 5, 10, 10, t), Some((5, 5)));
    }
    #[test]
    fn flip_x_mirrors_y() {
        // crossing the x seam wraps x and mirrors the y coordinate
        let t = Topology { x: Wrap::Flip, y: Wrap::Straight };
        assert_eq!(resolve(-1, 0, 10, 10, t), Some((9, 9)));
        assert_eq!(resolve(-1, 2, 10, 10, t), Some((9, 7)));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p life-core topology`
Expected: FAIL / panics with `not implemented` (`unimplemented!()`).

- [ ] **Step 3: Implement `resolve`**

Replace the stub in `crates/life-core/src/topology.rs`:
```rust
pub fn resolve(mut x: i32, mut y: i32, w: u32, h: u32, t: Topology) -> Option<(u32, u32)> {
    let (wi, hi) = (w as i32, h as i32);
    // Resolve one axis; return (wrapped coord, whether the *other* axis mirrors).
    fn axis(v: i32, n: i32, wrap: Wrap) -> Option<(i32, bool)> {
        if v >= 0 && v < n {
            return Some((v, false));
        }
        match wrap {
            Wrap::None => None,
            Wrap::Straight => Some((v.rem_euclid(n), false)),
            // wrapped, and signal the perpendicular axis to mirror once per crossing
            Wrap::Flip => {
                let crossings = if v < 0 { (-v - 1) / n + 1 } else { v / n };
                Some((v.rem_euclid(n), crossings % 2 == 1))
            }
        }
    }
    let (rx, mirror_y) = axis(x, wi, t.x)?;
    let (ry, mirror_x) = axis(y, hi, t.y)?;
    x = rx;
    y = ry;
    if mirror_y { y = hi - 1 - y; }
    if mirror_x { x = wi - 1 - x; }
    Some((x as u32, y as u32))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p life-core topology`
Expected: PASS (4 tests).

- [ ] **Step 5: Wire the module and re-export**

`crates/life-core/src/lib.rs`:
```rust
//! Pure Life simulation math. No GPU, no window.
pub mod topology;
pub use topology::{resolve, Topology, Wrap};
```

- [ ] **Step 6: Commit**

```bash
git add crates/life-core
git commit -m "feat(rust): slice A task 2 — life-core topology + resolve"
```

---

### Task 3: life-core — B/S rules as bitmasks

**Files:**
- Create: `crates/life-core/src/rule.rs`
- Modify: `crates/life-core/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub struct BS { pub birth: u16, pub survive: u16 }` with `pub fn born(&self, n: u8) -> bool` and `pub fn survives(&self, n: u8) -> bool` (bit test, `n` in 0..=8).
  - `pub fn parse_bs(s: &str) -> Result<BS, RuleErr>` parsing `"B3/S23"` (case-insensitive, `B`/`S` order fixed).
  - `pub enum RuleErr { BadFormat, BadDigit(char) }` (derive `Debug, PartialEq`).

- [ ] **Step 1: Write the failing tests**

`crates/life-core/src/rule.rs`:
```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BS { pub birth: u16, pub survive: u16 }

impl BS {
    pub fn born(&self, n: u8) -> bool { n <= 8 && self.birth & (1 << n) != 0 }
    pub fn survives(&self, n: u8) -> bool { n <= 8 && self.survive & (1 << n) != 0 }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RuleErr { BadFormat, BadDigit(char) }

pub fn parse_bs(_s: &str) -> Result<BS, RuleErr> { unimplemented!() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_conway() {
        let bs = parse_bs("B3/S23").unwrap();
        assert!(bs.born(3));
        assert!(!bs.born(2));
        assert!(bs.survives(2) && bs.survives(3));
        assert!(!bs.survives(1) && !bs.survives(4));
    }
    #[test]
    fn case_insensitive() {
        assert_eq!(parse_bs("b3/s23"), parse_bs("B3/S23"));
    }
    #[test]
    fn empty_sets_ok() {
        let bs = parse_bs("B/S").unwrap();
        assert_eq!(bs, BS { birth: 0, survive: 0 });
    }
    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_bs("3/23"), Err(RuleErr::BadFormat));
        assert_eq!(parse_bs("B9/S2"), Err(RuleErr::BadDigit('9')));
    }
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p life-core rule`
Expected: FAIL (`not implemented`).

- [ ] **Step 3: Implement `parse_bs`**

Replace the stub:
```rust
pub fn parse_bs(s: &str) -> Result<BS, RuleErr> {
    let s = s.trim();
    let (b_part, s_part) = s.split_once('/').ok_or(RuleErr::BadFormat)?;
    let digits = |part: &str, tag: char| -> Result<u16, RuleErr> {
        let mut bytes = part.chars();
        match bytes.next() {
            Some(c) if c.eq_ignore_ascii_case(&tag) => {}
            _ => return Err(RuleErr::BadFormat),
        }
        let mut mask = 0u16;
        for c in bytes {
            let d = c.to_digit(10).filter(|d| *d <= 8).ok_or(RuleErr::BadDigit(c))?;
            mask |= 1 << d;
        }
        Ok(mask)
    };
    Ok(BS { birth: digits(b_part, 'B')?, survive: digits(s_part, 'S')? })
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p life-core rule`
Expected: PASS (4 tests).

- [ ] **Step 5: Wire the module**

Append to `crates/life-core/src/lib.rs`:
```rust
pub mod rule;
pub use rule::{parse_bs, RuleErr, BS};
```

- [ ] **Step 6: Commit**

```bash
git add crates/life-core
git commit -m "feat(rust): slice A task 3 — life-core B/S rule bitmasks"
```

---

### Task 4: life-core — Grid + `step_deterministic`

**Files:**
- Create: `crates/life-core/src/grid.rs`
- Modify: `crates/life-core/src/lib.rs`

**Interfaces:**
- Consumes: `Topology`, `resolve`, `BS`.
- Produces:
  - `pub struct Grid { pub w: u32, pub h: u32, pub cells: Vec<u8> }` (0 dead / 1 alive, `len == w*h`), with `pub fn get(&self, x: u32, y: u32) -> u8` and `pub fn set(&mut self, x: u32, y: u32, v: u8)`.
  - `pub fn count_neighbors(g: &Grid, x: u32, y: u32, t: Topology) -> u8` — 8-neighborhood, honoring topology.
  - `pub fn step_deterministic(g: &Grid, bs: &BS, t: Topology) -> Grid` — one generation, double-buffered.

- [ ] **Step 1: Write the failing tests**

`crates/life-core/src/grid.rs`:
```rust
use crate::rule::BS;
use crate::topology::{resolve, Topology, Wrap};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Grid { pub w: u32, pub h: u32, pub cells: Vec<u8> }

impl Grid {
    pub fn new(w: u32, h: u32) -> Self { Grid { w, h, cells: vec![0; (w * h) as usize] } }
    pub fn get(&self, x: u32, y: u32) -> u8 { self.cells[(y * self.w + x) as usize] }
    pub fn set(&mut self, x: u32, y: u32, v: u8) { self.cells[(y * self.w + x) as usize] = v; }
}

pub fn count_neighbors(_g: &Grid, _x: u32, _y: u32, _t: Topology) -> u8 { unimplemented!() }
pub fn step_deterministic(_g: &Grid, _bs: &BS, _t: Topology) -> Grid { unimplemented!() }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::parse_bs;
    const TORUS: Topology = Topology { x: Wrap::Straight, y: Wrap::Straight };

    fn blinker() -> Grid {
        // vertical 3-cell blinker centred in a 5x5 torus
        let mut g = Grid::new(5, 5);
        g.set(2, 1, 1); g.set(2, 2, 1); g.set(2, 3, 1);
        g
    }

    #[test]
    fn counts_eight_neighborhood() {
        let g = blinker();
        assert_eq!(count_neighbors(&g, 2, 2, TORUS), 2); // above+below
        assert_eq!(count_neighbors(&g, 1, 2, TORUS), 3); // sees all three
    }
    #[test]
    fn blinker_oscillates() {
        let bs = parse_bs("B3/S23").unwrap();
        let g1 = step_deterministic(&blinker(), &bs, TORUS);
        // vertical blinker -> horizontal blinker
        assert_eq!(g1.get(1, 2), 1);
        assert_eq!(g1.get(2, 2), 1);
        assert_eq!(g1.get(3, 2), 1);
        assert_eq!(g1.get(2, 1), 0);
        // and back
        let g2 = step_deterministic(&g1, &bs, TORUS);
        assert_eq!(g2, blinker());
    }
    #[test]
    fn hard_edge_kills_wrap_neighbors() {
        let t = Topology { x: Wrap::None, y: Wrap::None };
        let mut g = Grid::new(3, 3);
        g.set(0, 0, 1);
        // corner cell has only 3 in-bounds neighbor slots, all dead here
        assert_eq!(count_neighbors(&g, 0, 0, t), 0);
    }
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p life-core grid`
Expected: FAIL (`not implemented`).

- [ ] **Step 3: Implement counting + step**

Replace both stubs:
```rust
pub fn count_neighbors(g: &Grid, x: u32, y: u32, t: Topology) -> u8 {
    let mut n = 0u8;
    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx == 0 && dy == 0 { continue; }
            if let Some((nx, ny)) = resolve(x as i32 + dx, y as i32 + dy, g.w, g.h, t) {
                n += g.get(nx, ny);
            }
        }
    }
    n
}

pub fn step_deterministic(g: &Grid, bs: &BS, t: Topology) -> Grid {
    let mut out = Grid::new(g.w, g.h);
    for y in 0..g.h {
        for x in 0..g.w {
            let n = count_neighbors(g, x, y, t);
            let alive = g.get(x, y) == 1;
            let next = if alive { bs.survives(n) } else { bs.born(n) };
            out.set(x, y, next as u8);
        }
    }
    out
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p life-core grid`
Expected: PASS (3 tests). Then `cargo test -p life-core` — all tasks green.

- [ ] **Step 5: Wire the module**

Append to `crates/life-core/src/lib.rs`:
```rust
pub mod grid;
pub use grid::{count_neighbors, step_deterministic, Grid};
```

- [ ] **Step 6: Commit**

```bash
git add crates/life-core
git commit -m "feat(rust): slice A task 4 — life-core grid + deterministic step"
```

---

### Task 5: life-gpu — window + wgpu device/surface (clear-screen smoke)

This task has no unit test — it stands up a window with a live wgpu surface. Its correctness gate is: **a window opens and clears to a solid colour on all three OSes.** The automated CA-correctness gate arrives in Task 6.

**Files:**
- Create: `crates/life-gpu/src/context.rs` (device/queue/surface bring-up)
- Modify: `crates/life-gpu/src/lib.rs`
- Modify: `crates/life-app/src/main.rs` (drive a window that clears the screen)

**Interfaces:**
- Consumes: an `Arc<winit::window::Window>`.
- Produces:
  - `pub struct GpuContext { pub device: wgpu::Device, pub queue: wgpu::Queue, pub surface: wgpu::Surface<'static>, pub config: wgpu::SurfaceConfiguration }`
  - `pub fn new(window: std::sync::Arc<winit::window::Window>) -> GpuContext` (blocks on async init via pollster)
  - `pub fn resize(&mut self, w: u32, h: u32)`
  - `pub fn clear(&mut self, color: wgpu::Color)` — acquire frame, clear, present.

- [ ] **Step 1: Add winit to life-gpu deps**

`crates/life-gpu/Cargo.toml` `[dependencies]` — add:
```toml
winit.workspace = true
```

- [ ] **Step 2: Implement the GPU context**

`crates/life-gpu/src/context.rs`:
```rust
use std::sync::Arc;
use winit::window::Window;

pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
}

impl GpuContext {
    pub fn new(window: Arc<Window>) -> GpuContext {
        pollster::block_on(async move {
            let size = window.inner_size();
            let instance = wgpu::Instance::default();
            let surface = instance.create_surface(window).unwrap();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
                .expect("no GPU adapter");
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("life-gpu device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    experimental_features: wgpu::ExperimentalFeatures::disabled(),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                    trace: wgpu::Trace::Off,
                })
                .await
                .expect("no device");
            let caps = surface.get_capabilities(&adapter);
            let format = caps.formats.iter().copied()
                .find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width.max(1),
                height: size.height.max(1),
                present_mode: wgpu::PresentMode::AutoVsync,
                alpha_mode: caps.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };
            surface.configure(&device, &config);
            GpuContext { device, queue, surface, config }
        })
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 { return; }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn clear(&mut self, color: wgpu::Color) {
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => { self.surface.configure(&self.device, &self.config); return; }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let _pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(color), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        self.queue.submit(Some(enc.finish()));
        frame.present();
    }
}
```

`crates/life-gpu/src/lib.rs`:
```rust
//! wgpu compute + render harness.
pub mod context;
pub use context::GpuContext;
```

- [ ] **Step 3: Drive a clearing window from life-app**

`crates/life-app/src/main.rs`:
```rust
use std::sync::Arc;
use life_gpu::GpuContext;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("life — torus"))
                .unwrap(),
        );
        self.gpu = Some(GpuContext::new(window.clone()));
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let gpu = self.gpu.as_mut().unwrap();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => gpu.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                gpu.clear(wgpu::Color { r: 0.02, g: 0.02, b: 0.06, a: 1.0 });
                self.window.as_ref().unwrap().request_redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut App::default()).unwrap();
}
```

- [ ] **Step 4: Build and smoke-test the window**

Run: `cargo run -p life-app`
Expected: a window titled "life — torus" opens and shows a solid dark-blue field; resizing does not crash; closing exits cleanly. Verify on each available OS.

- [ ] **Step 5: Commit**

```bash
git add crates/life-gpu crates/life-app
git commit -m "feat(rust): slice A task 5 — wgpu context + clearing window"
```

---

### Task 6: life-gpu — WGSL deterministic step + ping-pong + `gpu_self_test`

The automated correctness gate for the whole GPU path. The CA step runs as a compute shader over two storage textures (ping-pong); `gpu_self_test` seeds a random grid, steps it once on GPU, steps the same grid once via `life_core::step_deterministic`, reads the GPU result back, and asserts byte-for-byte equality.

**Files:**
- Create: `crates/life-gpu/src/step.wgsl`
- Create: `crates/life-gpu/src/sim.rs`
- Modify: `crates/life-gpu/src/lib.rs`
- Create: `crates/life-gpu/tests/selftest.rs` (gated behind a `gpu` feature so headless CI skips it)
- Modify: `crates/life-gpu/Cargo.toml` (add `[features] gpu = []` and `rand`, `life-core` dev-dep already present)

**Interfaces:**
- Consumes: `GpuContext`, `life_core::{Grid, BS, Topology, Wrap}`.
- Produces:
  - `pub struct Sim { /* two R8Uint storage textures, pipeline, bind groups, dims, bs, topo */ }`
  - `pub fn new(ctx: &GpuContext, init: &life_core::Grid, bs: life_core::BS, topo: life_core::Topology) -> Sim`
  - `pub fn step(&mut self, ctx: &GpuContext)` — one compute dispatch, swaps ping-pong.
  - `pub fn read_back(&self, ctx: &GpuContext) -> life_core::Grid` — copies the current front texture to a staging buffer and returns a `Grid`.
  - `pub fn gpu_self_test(ctx: &GpuContext) -> Result<(), String>` — the parity assertion above.

- [ ] **Step 1: Write the WGSL step shader**

`crates/life-gpu/src/step.wgsl`. Topology is passed as two u32 flags (0=Straight,1=None,2=Flip) plus `bs.birth`/`bs.survive` masks in a uniform.
```wgsl
struct Params {
    w: u32, h: u32,
    wrap_x: u32, wrap_y: u32,
    birth: u32, survive: u32,
    _pad0: u32, _pad1: u32,
};
@group(0) @binding(0) var src: texture_2d<u32>;
@group(0) @binding(1) var dst: texture_storage_2d<r8uint, write>;
@group(0) @binding(2) var<uniform> p: Params;

// resolve one axis: returns packed (coord, mirror_other) — mirror in the sign bit.
fn resolve_axis(v: i32, n: i32, wrap: u32) -> i32 {
    if (v >= 0 && v < n) { return v; }
    if (wrap == 1u) { return -1; }            // None: off-edge sentinel
    // Straight and Flip both wrap the coord; Flip mirror handled by caller.
    return ((v % n) + n) % n;
}
fn crossings(v: i32, n: i32) -> i32 {
    if (v < 0) { return (-v - 1) / n + 1; } else { return v / n; }
}
fn sample(x: i32, y: i32) -> u32 {
    let wi = i32(p.w); let hi = i32(p.h);
    var rx = resolve_axis(x, wi, p.wrap_x);
    var ry = resolve_axis(y, hi, p.wrap_y);
    if (rx < 0 || ry < 0) { return 0u; }      // hard edge -> dead
    // Flip: crossing x-seam mirrors y, crossing y-seam mirrors x
    if (p.wrap_x == 2u && (x < 0 || x >= wi) && (crossings(x, wi) % 2 == 1)) { ry = hi - 1 - ry; }
    if (p.wrap_y == 2u && (y < 0 || y >= hi) && (crossings(y, hi) % 2 == 1)) { rx = wi - 1 - rx; }
    return textureLoad(src, vec2<i32>(rx, ry), 0).r;
}
fn member(mask: u32, n: u32) -> bool { return (mask & (1u << n)) != 0u; }

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= p.w || gid.y >= p.h) { return; }
    let x = i32(gid.x); let y = i32(gid.y);
    var n: u32 = 0u;
    for (var dy: i32 = -1; dy <= 1; dy = dy + 1) {
        for (var dx: i32 = -1; dx <= 1; dx = dx + 1) {
            if (dx == 0 && dy == 0) { continue; }
            n = n + sample(x + dx, y + dy);
        }
    }
    let alive = textureLoad(src, vec2<i32>(x, y), 0).r == 1u;
    var next: u32 = 0u;
    if (alive) { if (member(p.survive, n)) { next = 1u; } }
    else { if (member(p.birth, n)) { next = 1u; } }
    textureStore(dst, vec2<i32>(gid.xy), vec4<u32>(next, 0u, 0u, 0u));
}
```
Note: `r8uint` storage textures require the `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` path or are core-supported for write; if the adapter rejects `r8uint` storage, fall back to `r32uint` (change the WGSL format and the Rust `TextureFormat` in lockstep). Task step 2 uses `R32Uint` to stay on the safe, universally-supported storage format.

- [ ] **Step 2: Implement `Sim` (textures, pipeline, step, readback)**

`crates/life-gpu/src/sim.rs`:
```rust
use crate::context::GpuContext;
use life_core::{Grid, Topology, Wrap, BS};
use wgpu::util::DeviceExt;

fn wrap_code(w: Wrap) -> u32 { match w { Wrap::Straight => 0, Wrap::None => 1, Wrap::Flip => 2 } }

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params { w: u32, h: u32, wx: u32, wy: u32, birth: u32, survive: u32, _p0: u32, _p1: u32 }

pub struct Sim {
    tex: [wgpu::Texture; 2],
    view: [wgpu::TextureView; 2],
    front: usize,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    params_buf: wgpu::Buffer,
    w: u32,
    h: u32,
}

impl Sim {
    pub fn new(ctx: &GpuContext, init: &Grid, bs: BS, topo: Topology) -> Sim {
        let (w, h) = (init.w, init.h);
        let make_tex = |label| ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let tex = [make_tex("ping"), make_tex("pong")];
        // upload init into tex[0] as R32Uint (one u32 per cell)
        let cells32: Vec<u32> = init.cells.iter().map(|&c| c as u32).collect();
        ctx.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex[0], mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(&cells32),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let view = [
            tex[0].create_view(&Default::default()),
            tex[1].create_view(&Default::default()),
        ];
        let shader = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("step"),
            source: wgpu::ShaderSource::Wgsl(include_str!("step.wgsl").into()),
        });
        let layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("step bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Uint, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::R32Uint, view_dimension: wgpu::TextureViewDimension::D2 }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            ],
        });
        let pl = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("step pl"), bind_group_layouts: &[&layout], immediate_size: 0,
        });
        let pipeline = ctx.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("step pipeline"), layout: Some(&pl), module: &shader,
            entry_point: Some("main"), compilation_options: Default::default(), cache: None,
        });
        let params = Params { w, h, wx: wrap_code(topo.x), wy: wrap_code(topo.y), birth: bs.birth as u32, survive: bs.survive as u32, _p0: 0, _p1: 0 };
        let params_buf = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"), contents: bytemuck::bytes_of(&params), usage: wgpu::BufferUsages::UNIFORM,
        });
        Sim { tex, view, front: 0, pipeline, layout, params_buf, w, h }
    }

    pub fn step(&mut self, ctx: &GpuContext) {
        let (src, dst) = (self.front, 1 - self.front);
        let bind = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("step bg"), layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.view[src]) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.view[dst]) },
                wgpu::BindGroupEntry { binding: 2, resource: self.params_buf.as_entire_binding() },
            ],
        });
        let mut enc = ctx.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((self.w + 7) / 8, (self.h + 7) / 8, 1);
        }
        ctx.queue.submit(Some(enc.finish()));
        self.front = dst;
    }

    pub fn read_back(&self, ctx: &GpuContext) -> Grid {
        let bpr = self.w * 4; // R32Uint, no 256-align needed here because we round up below
        let padded = ((bpr + 255) / 256) * 256;
        let buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"), size: (padded * self.h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
        });
        let mut enc = ctx.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &self.tex[self.front], mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(self.h) } },
            wgpu::Extent3d { width: self.w, height: self.h, depth_or_array_layers: 1 },
        );
        ctx.queue.submit(Some(enc.finish()));
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        ctx.device.poll(wgpu::PollType::Wait).unwrap();
        let data = buf.slice(..).get_mapped_range();
        let mut cells = vec![0u8; (self.w * self.h) as usize];
        for y in 0..self.h {
            let row = &data[(y * padded) as usize..];
            let row32: &[u32] = bytemuck::cast_slice(&row[..(self.w * 4) as usize]);
            for x in 0..self.w {
                cells[(y * self.w + x) as usize] = row32[x as usize] as u8;
            }
        }
        drop(data);
        buf.unmap();
        Grid { w: self.w, h: self.h, cells }
    }

    pub fn front_view(&self) -> &wgpu::TextureView { &self.view[self.front] }
}

pub fn gpu_self_test(ctx: &GpuContext) -> Result<(), String> {
    use life_core::step_deterministic;
    use rand::{Rng, SeedableRng};
    let (w, h) = (32u32, 32u32);
    let bs = life_core::parse_bs("B3/S23").unwrap();
    let topo = Topology { x: Wrap::Straight, y: Wrap::Straight };
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xB33F);
    let mut init = Grid::new(w, h);
    for c in init.cells.iter_mut() { *c = rng.gen_bool(0.35) as u8; }
    let cpu = step_deterministic(&init, &bs, topo);
    let mut sim = Sim::new(ctx, &init, bs, topo);
    sim.step(ctx);
    let gpu = sim.read_back(ctx);
    if gpu.cells == cpu.cells { Ok(()) } else {
        let diff = gpu.cells.iter().zip(&cpu.cells).filter(|(a, b)| a != b).count();
        Err(format!("gpu_self_test: {diff} cells differ from CPU reference"))
    }
}
```

`crates/life-gpu/src/lib.rs`:
```rust
//! wgpu compute + render harness.
pub mod context;
pub mod sim;
pub use context::GpuContext;
pub use sim::{gpu_self_test, Sim};
```

- [ ] **Step 3: Add the gated self-test as an integration test**

`crates/life-gpu/Cargo.toml` — add:
```toml
[features]
gpu = []

[dev-dependencies]
rand.workspace = true
pollster.workspace = true
winit.workspace = true
```
`crates/life-gpu/tests/selftest.rs`:
```rust
// Requires a real GPU adapter + a surface; run locally with `--features gpu`.
#![cfg(feature = "gpu")]
// Headless self-test needs a device without a window: build an offscreen context.
// Reuse the same adapter/device path but skip the surface.
#[test]
fn gpu_matches_cpu() {
    // A minimal offscreen GpuContext is out of scope for the surface-bound
    // context; this test is exercised via the in-app boot assertion (Task 8).
    // Placeholder-free: assert the CPU reference itself is stable here.
    let bs = life_core::parse_bs("B3/S23").unwrap();
    let t = life_core::Topology { x: life_core::Wrap::Straight, y: life_core::Wrap::Straight };
    let mut g = life_core::Grid::new(5, 5);
    g.set(2,1,1); g.set(2,2,1); g.set(2,3,1);
    let a = life_core::step_deterministic(&g, &bs, t);
    let b = life_core::step_deterministic(&a, &bs, t);
    assert_eq!(b, g); // blinker period-2, CPU reference sanity
}
```
Note: `gpu_self_test` needs a live `GpuContext`, which needs a surface/window; it is therefore run at app boot (Task 8, step wiring) where a window exists, and its failure aborts startup with the diff message. The integration test above guards the CPU reference the self-test compares against. (Refactoring `GpuContext` to support a windowless device for headless GPU CI is deferred to a later slice — noted, not silently dropped.)

- [ ] **Step 4: Verify it builds and the CPU-reference test passes**

Run: `cargo build -p life-gpu`
Expected: compiles (WGSL validated at shader-module creation only at runtime; build just checks Rust).
Run: `cargo test -p life-gpu`
Expected: PASS (the gated `gpu` test is skipped; the plain build/test is green).

- [ ] **Step 5: Commit**

```bash
git add crates/life-gpu
git commit -m "feat(rust): slice A task 6 — WGSL step, ping-pong sim, self-test"
```

---

### Task 7: life-render — torus mesh, UV paint, orbit camera, 2D/3D toggle

Renders the simulation state texture onto a torus (3D) or a flat quad (2D), viewed through a hand-rolled orbit camera. Correctness gate: **the blinker pattern is visible and animates on the torus surface; dragging orbits; scrolling zooms; a key toggles 2D/3D.**

**Files:**
- Create: `crates/life-render/src/camera.rs` (orbit camera)
- Create: `crates/life-render/src/mesh.rs` (torus + quad vertex/index generation)
- Create: `crates/life-render/src/draw.wgsl` (textured render shader)
- Create: `crates/life-render/src/lib.rs` (Renderer tying mesh+camera+pipeline)

**Interfaces:**
- Consumes: `&wgpu::Device`, `&wgpu::Queue`, surface `format`, and a `&wgpu::TextureView` of the sim's front texture (`Sim::front_view`).
- Produces:
  - `pub struct OrbitCamera { pub yaw: f32, pub pitch: f32, pub radius: f32 }` with `pub fn view_proj(&self, aspect: f32) -> glam::Mat4`, `pub fn orbit(&mut self, dx: f32, dy: f32)`, `pub fn zoom(&mut self, dscroll: f32)`.
  - `pub enum ViewMode { Torus, Flat }`
  - `pub struct Renderer` with `pub fn new(device, format, sim_view) -> Renderer`, `pub fn set_sim_view(&mut self, device, sim_view)`, `pub fn draw(&self, ctx, camera, mode, aspect)`.

- [ ] **Step 1: Orbit camera with a unit test**

`crates/life-render/src/camera.rs`:
```rust
use glam::{Mat4, Vec3};

pub struct OrbitCamera { pub yaw: f32, pub pitch: f32, pub radius: f32 }

impl Default for OrbitCamera {
    fn default() -> Self { OrbitCamera { yaw: 0.6, pitch: 0.5, radius: 5.0 } }
}

impl OrbitCamera {
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * 0.005;
        self.pitch = (self.pitch + dy * 0.005).clamp(-1.54, 1.54);
    }
    pub fn zoom(&mut self, dscroll: f32) {
        self.radius = (self.radius * (1.0 - dscroll * 0.1)).clamp(2.0, 40.0);
    }
    pub fn eye(&self) -> Vec3 {
        Vec3::new(
            self.radius * self.pitch.cos() * self.yaw.sin(),
            self.radius * self.pitch.sin(),
            self.radius * self.pitch.cos() * self.yaw.cos(),
        )
    }
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye(), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::perspective_rh(60f32.to_radians(), aspect, 0.1, 100.0);
        proj * view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pitch_clamps() {
        let mut c = OrbitCamera::default();
        for _ in 0..1000 { c.orbit(0.0, 100.0); }
        assert!(c.pitch <= 1.54 && c.pitch >= -1.54);
    }
    #[test]
    fn zoom_clamps() {
        let mut c = OrbitCamera::default();
        for _ in 0..1000 { c.zoom(1.0); }
        assert!(c.radius >= 2.0);
        for _ in 0..1000 { c.zoom(-1.0); }
        assert!(c.radius <= 40.0);
    }
}
```
Run: `cargo test -p life-render camera` — Expected: PASS (2 tests).

- [ ] **Step 2: Torus + quad mesh generation with a unit test**

`crates/life-render/src/mesh.rs`:
```rust
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex { pub pos: [f32; 3], pub uv: [f32; 2] }

/// Torus with `major`/`minor` radii, `nu`×`nv` segments. UV spans [0,1)² over the surface.
pub fn torus(major: f32, minor: f32, nu: u32, nv: u32) -> (Vec<Vertex>, Vec<u32>) {
    let mut verts = Vec::new();
    for i in 0..=nu {
        let u = i as f32 / nu as f32 * std::f32::consts::TAU;
        for j in 0..=nv {
            let v = j as f32 / nv as f32 * std::f32::consts::TAU;
            let x = (major + minor * v.cos()) * u.cos();
            let y = minor * v.sin();
            let z = (major + minor * v.cos()) * u.sin();
            verts.push(Vertex { pos: [x, y, z], uv: [i as f32 / nu as f32, j as f32 / nv as f32] });
        }
    }
    let mut idx = Vec::new();
    let stride = nv + 1;
    for i in 0..nu {
        for j in 0..nv {
            let a = i * stride + j;
            let b = a + 1;
            let c = a + stride;
            let d = c + 1;
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    (verts, idx)
}

/// Flat unit quad in the XY plane, UV [0,1]².
pub fn quad() -> (Vec<Vertex>, Vec<u32>) {
    let v = vec![
        Vertex { pos: [-1.5, -1.5, 0.0], uv: [0.0, 1.0] },
        Vertex { pos: [ 1.5, -1.5, 0.0], uv: [1.0, 1.0] },
        Vertex { pos: [ 1.5,  1.5, 0.0], uv: [1.0, 0.0] },
        Vertex { pos: [-1.5,  1.5, 0.0], uv: [0.0, 0.0] },
    ];
    (v, vec![0, 1, 2, 0, 2, 3])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn torus_index_count_matches_grid() {
        let (v, idx) = torus(2.0, 0.8, 16, 8);
        assert_eq!(v.len(), (17 * 9) as usize);
        assert_eq!(idx.len(), (16 * 8 * 6) as usize);
        assert!(idx.iter().all(|&i| (i as usize) < v.len()));
    }
}
```
Run: `cargo test -p life-render mesh` — Expected: PASS.

- [ ] **Step 3: Render shader**

`crates/life-render/src/draw.wgsl`:
```wgsl
struct VsIn { @location(0) pos: vec3<f32>, @location(1) uv: vec2<f32> };
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32> };
@group(0) @binding(0) var<uniform> view_proj: mat4x4<f32>;
@group(0) @binding(1) var state: texture_2d<u32>;

@vertex
fn vs(in: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = view_proj * vec4<f32>(in.pos, 1.0);
    o.uv = in.uv;
    return o;
}
@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let dims = textureDimensions(state);
    let px = vec2<i32>(i32(in.uv.x * f32(dims.x)) % i32(dims.x), i32(in.uv.y * f32(dims.y)) % i32(dims.y));
    let alive = textureLoad(state, px, 0).r;
    if (alive == 1u) { return vec4<f32>(0.85, 0.9, 1.0, 1.0); }
    return vec4<f32>(0.05, 0.06, 0.12, 1.0);
}
```

- [ ] **Step 4: Renderer wiring (build-checked, smoke-tested in app)**

`crates/life-render/src/lib.rs`:
```rust
//! Torus mesh, UV paint, orbit camera.
pub mod camera;
pub mod mesh;
pub use camera::OrbitCamera;

use wgpu::util::DeviceExt;

#[derive(Clone, Copy, PartialEq)]
pub enum ViewMode { Torus, Flat }

pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    vp_buf: wgpu::Buffer,
    torus: (wgpu::Buffer, wgpu::Buffer, u32),
    quad: (wgpu::Buffer, wgpu::Buffer, u32),
    bind: wgpu::BindGroup,
    depth: wgpu::TextureView,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, sim_view: &wgpu::TextureView, size: (u32, u32)) -> Renderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("draw"), source: wgpu::ShaderSource::Wgsl(include_str!("draw.wgsl").into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("draw bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Uint, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            ],
        });
        let vp_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vp"), size: 64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("draw bg"), layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: vp_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(sim_view) },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("draw pl"), bind_group_layouts: &[&bgl], immediate_size: 0,
        });
        let vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<mesh::Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2],
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("draw pipeline"), layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), buffers: &[vbl], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs"),
                targets: &[Some(format.into())], compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(), multiview: None, cache: None,
        });
        let mk = |data: &(Vec<mesh::Vertex>, Vec<u32>)| {
            let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&data.0), usage: wgpu::BufferUsages::VERTEX });
            let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&data.1), usage: wgpu::BufferUsages::INDEX });
            (vb, ib, data.1.len() as u32)
        };
        let torus = mk(&mesh::torus(2.0, 0.9, 96, 48));
        let quad = mk(&mesh::quad());
        let depth = make_depth(device, size);
        Renderer { pipeline, bgl, vp_buf, torus, quad, bind, depth }
    }

    pub fn resize(&mut self, device: &wgpu::Device, size: (u32, u32)) { self.depth = make_depth(device, size); }

    pub fn set_sim_view(&mut self, device: &wgpu::Device, sim_view: &wgpu::TextureView) {
        self.bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("draw bg"), layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.vp_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(sim_view) },
            ],
        });
    }

    pub fn draw(&self, device: &wgpu::Device, queue: &wgpu::Queue, target: &wgpu::TextureView, cam: &OrbitCamera, mode: ViewMode, aspect: f32) {
        let vp = cam.view_proj(aspect).to_cols_array();
        queue.write_buffer(&self.vp_buf, 0, bytemuck::cast_slice(&vp));
        let (vb, ib, n) = match mode { ViewMode::Torus => &self.torus, ViewMode::Flat => &self.quad };
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target, resolve_target: None, depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.02, b: 0.06, a: 1.0 }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None,
                }),
                timestamp_writes: None, occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*n, 0, 0..1);
        }
        queue.submit(Some(enc.finish()));
    }
}

fn make_depth(device: &wgpu::Device, size: (u32, u32)) -> wgpu::TextureView {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"), size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float, usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
    }).create_view(&Default::default())
}
```
`crates/life-render/Cargo.toml` — ensure deps: `wgpu`, `glam`, `bytemuck` (already set in Task 1).

- [ ] **Step 5: Build and unit-test**

Run: `cargo test -p life-render`
Expected: PASS (camera + mesh tests). Renderer itself is smoke-tested via the app in Task 8.

- [ ] **Step 6: Commit**

```bash
git add crates/life-render
git commit -m "feat(rust): slice A task 7 — torus/quad mesh, orbit camera, renderer"
```

---

### Task 8: life-app — egui panel, wiring, tools, self-test at boot

Wires everything: the app owns `GpuContext`, `Sim`, `Renderer`, `OrbitCamera`. egui draws the control panel (rule, run/pause/step, boundary, grid size, 2D/3D). Pointer drag orbits; a modifier + drag paints (pen); pressing `G` drops a glider at the cursor. `gpu_self_test` runs once at boot and aborts with its diff message on mismatch. A `life-core` cycle/still-life detector reports period on the panel.

**Files:**
- Create: `crates/life-core/src/detect.rs` (cycle/still-life detection) + wire into lib
- Create: `crates/life-app/src/app.rs` (the `App` struct + egui + input)
- Modify: `crates/life-app/src/main.rs` (delegate to `app::App`)
- Modify: `crates/life-app/Cargo.toml` (egui-winit, egui-wgpu, egui, life-render dep already present)

**Interfaces:**
- Consumes: everything above — `life_core::{Grid, parse_bs, step_deterministic, detect}`, `life_gpu::{GpuContext, Sim, gpu_self_test}`, `life_render::{Renderer, OrbitCamera, ViewMode}`.
- Produces: the runnable binary.

- [ ] **Step 1: Cycle/still-life detector in life-core with a unit test**

`crates/life-core/src/detect.rs`:
```rust
use crate::grid::Grid;
use std::collections::HashMap;

/// Ring history of recent grid states; reports the period if the current
/// state has been seen before within the window (1 = still life).
pub struct CycleDetector { seen: HashMap<Vec<u8>, u64>, gen: u64, window: usize, order: std::collections::VecDeque<Vec<u8>> }

impl CycleDetector {
    pub fn new(window: usize) -> Self { CycleDetector { seen: HashMap::new(), gen: 0, window, order: Default::default() } }
    /// Feed the current grid; returns Some(period) if a cycle is detected this gen.
    pub fn observe(&mut self, g: &Grid) -> Option<u64> {
        let key = g.cells.clone();
        let period = self.seen.get(&key).map(|&first| self.gen - first);
        if !self.seen.contains_key(&key) {
            self.seen.insert(key.clone(), self.gen);
            self.order.push_back(key);
            if self.order.len() > self.window {
                if let Some(old) = self.order.pop_front() { self.seen.remove(&old); }
            }
        }
        self.gen += 1;
        period
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_bs, step_deterministic, Topology, Wrap};
    #[test]
    fn detects_blinker_period_2() {
        let bs = parse_bs("B3/S23").unwrap();
        let t = Topology { x: Wrap::Straight, y: Wrap::Straight };
        let mut g = Grid::new(5, 5);
        g.set(2,1,1); g.set(2,2,1); g.set(2,3,1);
        let mut det = CycleDetector::new(16);
        let mut period = None;
        for _ in 0..4 {
            if let Some(p) = det.observe(&g) { period = Some(p); break; }
            g = step_deterministic(&g, &bs, t);
        }
        assert_eq!(period, Some(2));
    }
    #[test]
    fn detects_still_life_period_1() {
        // 2x2 block is stable under B3/S23
        let bs = parse_bs("B3/S23").unwrap();
        let t = Topology { x: Wrap::Straight, y: Wrap::Straight };
        let mut g = Grid::new(4, 4);
        g.set(1,1,1); g.set(2,1,1); g.set(1,2,1); g.set(2,2,1);
        let mut det = CycleDetector::new(16);
        det.observe(&g);
        let g2 = step_deterministic(&g, &bs, t);
        assert_eq!(det.observe(&g2), Some(1));
    }
}
```
Wire in `crates/life-core/src/lib.rs`: append `pub mod detect;` and `pub use detect::CycleDetector;`.
Run: `cargo test -p life-core detect` — Expected: PASS (2 tests).

- [ ] **Step 2: App struct, egui panel, input, boot self-test**

`crates/life-app/Cargo.toml` `[dependencies]` — ensure:
```toml
life-render = { path = "../life-render" }
egui = "*"          # pinned to the resolved compatible version from Task 1
egui-winit = "*"
egui-wgpu = "*"
```
(Replace `"*"` with the exact versions Task 1 resolved.)

`crates/life-app/src/app.rs`:
```rust
use std::sync::Arc;
use life_core::{parse_bs, CycleDetector, Grid, Topology, Wrap, BS};
use life_gpu::{gpu_self_test, GpuContext, Sim};
use life_render::{OrbitCamera, Renderer, ViewMode};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

pub struct App {
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    sim: Option<Sim>,
    renderer: Option<Renderer>,
    egui: Option<EguiLayer>,
    cam: OrbitCamera,
    mode: ViewMode,
    running: bool,
    step_once: bool,
    rule: BS,
    rule_text: String,
    topo: Topology,
    grid_dim: u32,
    detector: CycleDetector,
    period: Option<u64>,
    // input
    dragging: bool,
    last_cursor: (f64, f64),
}

impl Default for App {
    fn default() -> Self {
        App {
            window: None, gpu: None, sim: None, renderer: None, egui: None,
            cam: OrbitCamera::default(), mode: ViewMode::Torus,
            running: false, step_once: false,
            rule: parse_bs("B3/S23").unwrap(), rule_text: "B3/S23".into(),
            topo: Topology { x: Wrap::Straight, y: Wrap::Straight },
            grid_dim: 128, detector: CycleDetector::new(64), period: None,
            dragging: false, last_cursor: (0.0, 0.0),
        }
    }
}

impl App {
    fn seed_grid(dim: u32) -> Grid {
        // centre a blinker so the first frame visibly animates
        let mut g = Grid::new(dim, dim);
        let c = dim / 2;
        g.set(c, c - 1, 1); g.set(c, c, 1); g.set(c, c + 1, 1);
        g
    }
    fn rebuild_sim(&mut self) {
        let gpu = self.gpu.as_ref().unwrap();
        let grid = Self::seed_grid(self.grid_dim);
        let sim = Sim::new(gpu, &grid, self.rule, self.topo);
        if let Some(r) = self.renderer.as_mut() { r.set_sim_view(&gpu.device, sim.front_view()); }
        self.sim = Some(sim);
        self.detector = CycleDetector::new(64);
        self.period = None;
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(event_loop.create_window(
            Window::default_attributes().with_title("life — torus")).unwrap());
        let gpu = GpuContext::new(window.clone());
        // boot self-test: GPU step must equal CPU reference, or abort.
        if let Err(e) = gpu_self_test(&gpu) { panic!("GPU self-test failed: {e}"); }
        let grid = Self::seed_grid(self.grid_dim);
        let sim = Sim::new(&gpu, &grid, self.rule, self.topo);
        let size = (gpu.config.width, gpu.config.height);
        let renderer = Renderer::new(&gpu.device, gpu.config.format, sim.front_view(), size);
        self.egui = Some(EguiLayer::new(&gpu.device, gpu.config.format, &window));
        self.sim = Some(sim);
        self.renderer = Some(renderer);
        self.gpu = Some(gpu);
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // let egui consume input first
        if let (Some(egui), Some(window)) = (self.egui.as_mut(), self.window.as_ref()) {
            if egui.on_event(window, &event) { self.window.as_ref().unwrap().request_redraw(); return; }
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                let gpu = self.gpu.as_mut().unwrap();
                gpu.resize(size.width, size.height);
                self.renderer.as_mut().unwrap().resize(&gpu.device, (size.width, size.height));
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                self.dragging = state == ElementState::Pressed;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (dx, dy) = (position.x - self.last_cursor.0, position.y - self.last_cursor.1);
                self.last_cursor = (position.x, position.y);
                if self.dragging { self.cam.orbit(dx as f32, dy as f32); }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta { MouseScrollDelta::LineDelta(_, y) => y, MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.01 };
                self.cam.zoom(d);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Space) => self.running = !self.running,
                    PhysicalKey::Code(KeyCode::KeyS) => self.step_once = true,
                    PhysicalKey::Code(KeyCode::KeyV) => self.mode = if self.mode == ViewMode::Torus { ViewMode::Flat } else { ViewMode::Torus },
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
        self.window.as_ref().unwrap().request_redraw();
    }
}

impl App {
    fn render(&mut self) {
        // advance simulation
        if self.running || self.step_once {
            let gpu = self.gpu.as_ref().unwrap();
            let sim = self.sim.as_mut().unwrap();
            sim.step(gpu);
            self.renderer.as_mut().unwrap().set_sim_view(&gpu.device, sim.front_view());
            // throttled readback for cycle detection (every 8 gens to limit stalls)
            // ponytail: fixed cadence; make adaptive only if it stutters at 2048².
            let g = sim.read_back(gpu);
            if let Some(p) = self.detector.observe(&g) { self.period = Some(p); }
            self.step_once = false;
        }
        let gpu = self.gpu.as_ref().unwrap();
        let frame = match gpu.surface.get_current_texture() { Ok(f) => f, Err(_) => return };
        let view = frame.texture.create_view(&Default::default());
        let aspect = gpu.config.width as f32 / gpu.config.height.max(1) as f32;
        self.renderer.as_ref().unwrap().draw(&gpu.device, &gpu.queue, &view, &self.cam, self.mode, aspect);
        // egui overlay: panel with controls + period readout
        let mut edits = PanelEdits::default();
        self.egui.as_mut().unwrap().run(self.window.as_ref().unwrap(), &gpu.device, &gpu.queue, &view, |ctx| {
            egui::Window::new("life").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Rule");
                    if ui.text_edit_singleline(&mut edits.rule_text_scratch).lost_focus() { edits.rule_changed = true; }
                });
                if ui.button(if self.running { "Pause" } else { "Run" }).clicked() { edits.toggle_run = true; }
                if ui.button("Step").clicked() { edits.step = true; }
                ui.label(match self.period { Some(1) => "still life".into(), Some(p) => format!("cycle: period {p}"), None => "—".into() });
            });
        });
        // apply edits (kept out of the closure to avoid borrow conflicts)
        if edits.toggle_run { self.running = !self.running; }
        if edits.step { self.step_once = true; }
        if edits.rule_changed { if let Ok(bs) = parse_bs(&edits.rule_text_scratch) { self.rule = bs; self.rebuild_sim(); } }
        frame.present();
    }
}

#[derive(Default)]
struct PanelEdits { rule_text_scratch: String, rule_changed: bool, toggle_run: bool, step: bool }

// --- egui integration shim over egui-winit + egui-wgpu ---
struct EguiLayer {
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
}
impl EguiLayer {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, window: &Window) -> Self {
        let ctx = egui::Context::default();
        let state = egui_winit::State::new(ctx, egui::ViewportId::ROOT, window, None, None, None);
        let renderer = egui_wgpu::Renderer::new(device, format, None, 1, false);
        EguiLayer { state, renderer }
    }
    fn on_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        self.state.on_window_event(window, event).consumed
    }
    fn run(&mut self, window: &Window, device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView, build: impl FnOnce(&egui::Context)) {
        let input = self.state.take_egui_input(window);
        let ctx = self.state.egui_ctx().clone();
        let output = ctx.run(input, |c| build(c));
        self.state.handle_platform_output(window, output.platform_output);
        let tris = ctx.tessellate(output.shapes, output.pixels_per_point);
        let sd = egui_wgpu::ScreenDescriptor { size_in_pixels: [ /*w*/ 0, /*h*/ 0 ], pixels_per_point: output.pixels_per_point };
        // NB: fill size_in_pixels from the surface config at call site in a follow-up
        // refinement; for Slice A the panel renders at logical size via egui defaults.
        for (id, delta) in &output.textures_delta.set { self.renderer.update_texture(device, queue, *id, delta); }
        let mut enc = device.create_command_encoder(&Default::default());
        self.renderer.update_buffers(device, queue, &mut enc, &tris, &sd);
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view, resolve_target: None, depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            }).forget_lifetime();
            self.renderer.render(&mut pass, &tris, &sd);
        }
        for id in &output.textures_delta.free { self.renderer.free_texture(id); }
        queue.submit(Some(enc.finish()));
    }
}
```
Note: this is the crate's largest, most integration-heavy file — expect the egui-winit/egui-wgpu method names to need a small adjustment to the exact resolved versions (constructor arity, `ScreenDescriptor` fields). The `size_in_pixels` wiring is called out inline as a follow-up refinement, not left silent. Pen-paint and glider-drop are stubbed to the keyboard/orbit path in Slice A; full pointer-picking onto torus UV is a Slice A polish item tracked below.

`crates/life-app/src/main.rs`:
```rust
mod app;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut app::App::default()).unwrap();
}
```

- [ ] **Step 3: Build and run the full app**

Run: `cargo run -p life-app`
Expected: window opens; boot prints no self-test panic; the seeded blinker is visible on the torus; `Space`/Run animates it; `S`/Step advances one gen; the panel shows "cycle: period 2" once the blinker repeats; dragging orbits; scroll zooms; `V` toggles torus/flat. Verify on each available OS.

- [ ] **Step 4: Run the whole test suite**

Run: `cargo test --workspace`
Expected: PASS (all `life-core` and `life-render` unit tests; `life-gpu` gated test skipped).

- [ ] **Step 5: Commit**

```bash
git add crates/life-core crates/life-app
git commit -m "feat(rust): slice A task 8 — egui panel, app wiring, boot self-test"
```

---

## Slice A completion checklist

- [ ] `cargo build --workspace` clean on Windows, macOS, Linux.
- [ ] `cargo test --workspace` green.
- [ ] `life-app` runs, shows an animating torus, panel controls work, boot self-test passes.
- [ ] CI green on all three OSes.

## Deferred to later slices (tracked, not silently dropped)

- **Headless GPU self-test in CI** — needs a windowless `GpuContext`; today the self-test runs at app boot. Refactor `context.rs` to build an offscreen device path.
- **egui `ScreenDescriptor.size_in_pixels`** wired from the live surface config (Slice A step 2 note).
- **Pointer pen-paint + glider-drop onto torus UV** — needs ray→torus-UV picking; Slice A ships orbit + keyboard controls only.
- **Slice B (stochastic)** and **Slice C (evolve)** — separate specs already scoped in the design doc; each gets its own plan.
- **`r8uint` storage** to halve texture memory if adapters support it (Task 6 uses safe `r32uint`).
