//! Screen-point → grid-cell picking for the pen/glider/terrain tools.
//! Torus mode ray-marches the torus SDF; flat mode intersects the z=0 quad.

use crate::camera::OrbitCamera;
use crate::mesh::{MAJOR, MINOR, QUAD_HALF};
use crate::ViewMode;
use glam::{Vec3, Vec4};

/// Torus signed distance (axis = Y, same parametrisation as mesh::torus).
fn torus_sdf(p: Vec3) -> f32 {
    let q = glam::Vec2::new((p.x * p.x + p.z * p.z).sqrt() - MAJOR, p.y);
    q.length() - MINOR
}

/// Unproject an NDC point to a world-space ray from the camera eye.
fn ray(cam: &OrbitCamera, aspect: f32, ndc: (f32, f32)) -> (Vec3, Vec3) {
    let inv = cam.view_proj(aspect).inverse();
    let near = inv * Vec4::new(ndc.0, ndc.1, 0.0, 1.0);
    let far = inv * Vec4::new(ndc.0, ndc.1, 1.0, 1.0);
    let near = near.truncate() / near.w;
    let far = far.truncate() / far.w;
    (near, (far - near).normalize())
}

/// Fractional wrap of an angle into [0, 1).
fn angle_frac(a: f32) -> f32 {
    let f = a / std::f32::consts::TAU;
    f - f.floor()
}

/// Pick the grid cell under an NDC screen point, or None if the ray misses.
pub fn pick_cell(
    cam: &OrbitCamera,
    aspect: f32,
    ndc: (f32, f32),
    mode: ViewMode,
    dims: (u32, u32),
) -> Option<(u32, u32)> {
    let (origin, dir) = ray(cam, aspect, ndc);
    let (u, v) = match mode {
        ViewMode::Flat => {
            // quad in the XY plane at z=0; uv: u = (x+H)/2H, v = (H−y)/2H
            if dir.z.abs() < 1e-6 {
                return None;
            }
            let t = -origin.z / dir.z;
            if t <= 0.0 {
                return None;
            }
            let p = origin + dir * t;
            if p.x.abs() > QUAD_HALF || p.y.abs() > QUAD_HALF {
                return None;
            }
            ((p.x + QUAD_HALF) / (2.0 * QUAD_HALF), (QUAD_HALF - p.y) / (2.0 * QUAD_HALF))
        }
        ViewMode::Torus => {
            // sphere-trace the torus SDF
            let mut t = 0.0f32;
            let mut hit = None;
            for _ in 0..128 {
                let p = origin + dir * t;
                let d = torus_sdf(p);
                if d < 1e-3 {
                    hit = Some(p);
                    break;
                }
                t += d;
                if t > 100.0 {
                    break;
                }
            }
            let p = hit?;
            let u = angle_frac(p.z.atan2(p.x));
            let v = angle_frac(p.y.atan2((p.x * p.x + p.z * p.z).sqrt() - MAJOR));
            (u, v)
        }
    };
    let x = ((u * dims.0 as f32) as u32).min(dims.0 - 1);
    let y = ((v * dims.1 as f32) as u32).min(dims.1 - 1);
    Some((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_centre_picks_centre_cell() {
        // camera straight down +Z axis looking at origin: yaw 0, pitch 0 puts
        // the eye on +Z (see OrbitCamera::eye), quad faces it.
        let cam = OrbitCamera { yaw: 0.0, pitch: 0.0, radius: 5.0 };
        let cell = pick_cell(&cam, 1.0, (0.0, 0.0), ViewMode::Flat, (100, 100)).unwrap();
        assert_eq!(cell, (50, 50));
    }

    #[test]
    fn flat_off_quad_misses() {
        let cam = OrbitCamera { yaw: 0.0, pitch: 0.0, radius: 5.0 };
        // far corner of the screen at this fov/distance is off the quad
        assert_eq!(pick_cell(&cam, 1.0, (0.99, 0.99), ViewMode::Flat, (100, 100)), None);
    }

    #[test]
    fn torus_centre_screen_passes_through_hole() {
        // the camera orbits the origin — dead centre of the torus HOLE, so a
        // centre-screen ray misses; slightly below centre hits the near tube.
        let cam = OrbitCamera::default();
        assert_eq!(pick_cell(&cam, 1.6, (0.0, 0.0), ViewMode::Torus, (128, 128)), None);
        let cell = pick_cell(&cam, 1.6, (0.0, -0.5), ViewMode::Torus, (128, 128));
        assert!(cell.is_some(), "ray below centre must hit the near tube");
    }

    #[test]
    fn torus_screen_corner_misses() {
        let cam = OrbitCamera::default();
        // extreme corner ray passes the torus at radius 5 with 60° fov
        assert_eq!(pick_cell(&cam, 1.6, (0.999, 0.999), ViewMode::Torus, (128, 128)), None);
    }

    #[test]
    fn torus_uv_round_trip() {
        // a point ON the torus surface at known angles picks the matching cell
        let (u, v) = (0.25f32, 0.5f32); // z-axis side, inner equator
        let ang_u = u * std::f32::consts::TAU;
        let ang_v = v * std::f32::consts::TAU;
        let p = Vec3::new(
            (MAJOR + MINOR * ang_v.cos()) * ang_u.cos(),
            MINOR * ang_v.sin(),
            (MAJOR + MINOR * ang_v.cos()) * ang_u.sin(),
        );
        // camera slightly outside that point, looking through it toward origin
        assert!((torus_sdf(p)).abs() < 1e-5);
        let uu = angle_frac(p.z.atan2(p.x));
        let vv = angle_frac(p.y.atan2((p.x * p.x + p.z * p.z).sqrt() - MAJOR));
        assert!((uu - u).abs() < 1e-5 && (vv - v).abs() < 1e-5);
    }
}
