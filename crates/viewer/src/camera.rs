use glam::{Mat4, Vec3};

pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,   // radians, horizontal rotation
    pub pitch: f32, // radians, vertical rotation, clamped ±89°
}

impl Camera {
    pub fn new(position: Vec3) -> Self {
        Camera { position, yaw: 0.0, pitch: 0.0 }
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
    }

    pub fn right(&self) -> Vec3 {
        Vec3::Y.cross(self.forward()).normalize()
    }

    pub fn up(&self) -> Vec3 {
        self.right().cross(self.forward()).normalize()
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.position + self.forward(), Vec3::Y)
    }

    pub fn apply_mouse(&mut self, dx: f32, dy: f32, sensitivity: f32) {
        self.yaw += dx * sensitivity;
        self.pitch -= dy * sensitivity;
        self.pitch = self.pitch.clamp(
            -std::f32::consts::FRAC_PI_2 + 0.01,
            std::f32::consts::FRAC_PI_2 - 0.01,
        );
    }

    pub fn move_local(&mut self, fwd: f32, right: f32, up: f32, speed: f32) {
        let f = self.forward();
        let r = self.right();
        self.position += (f * fwd + r * right + Vec3::Y * up) * speed;
    }
}

pub fn projection_matrix(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    Mat4::perspective_rh(fov_y_rad, aspect, near, far)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    #[test]
    fn forward_default_points_along_x() {
        // yaw=0, pitch=0 → forward = (cos0·cos0, sin0, sin0·cos0) = (1, 0, 0) = +X
        let cam = Camera::new(Vec3::ZERO);
        let fwd = cam.forward();
        assert!(fwd.x.abs() > 0.99, "default forward should be ~+X: {fwd:?}");
        assert!(fwd.is_finite(), "non-finite forward");
    }

    #[test]
    fn right_is_perpendicular_to_forward() {
        let cam = Camera::new(Vec3::ZERO);
        let dot = cam.forward().dot(cam.right()).abs();
        assert!(dot < 1e-5, "right not perpendicular to forward: dot={dot}");
    }

    #[test]
    fn pitch_clamps() {
        let mut cam = Camera::new(Vec3::ZERO);
        cam.apply_mouse(0.0, 1e6, 1.0);
        assert!(cam.pitch < PI / 2.0, "pitch over-clamped");
    }

    #[test]
    fn view_proj_matrix_is_finite() {
        let cam = Camera::new(Vec3::new(0.0, 5.0, -20.0));
        let view = cam.view_matrix();
        let proj = projection_matrix(std::f32::consts::FRAC_PI_3, 16.0 / 9.0, 0.1, 2000.0);
        let vp = proj * view;
        for col in vp.to_cols_array() {
            assert!(col.is_finite(), "non-finite in view-proj: {col}");
        }
    }

    #[test]
    fn move_local_changes_position() {
        let mut cam = Camera::new(Vec3::ZERO);
        let before = cam.position;
        cam.move_local(1.0, 0.0, 0.0, 1.0);
        assert_ne!(cam.position, before);
    }
}
