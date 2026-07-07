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
