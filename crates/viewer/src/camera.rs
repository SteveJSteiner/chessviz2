use glam::{Mat4, Vec3};

/// Orbit camera: always looks at `focus`, positioned on a sphere of `radius`.
/// Azimuth rotates around world Y; elevation tilts up/down. Both in radians.
pub struct Camera {
    pub focus: Vec3,
    pub azimuth: f32,
    pub elevation: f32,
    pub radius: f32,
}

impl Camera {
    pub fn new(focus: Vec3, azimuth: f32, elevation: f32, radius: f32) -> Self {
        Camera { focus, azimuth, elevation, radius }
    }

    pub fn position(&self) -> Vec3 {
        let (sin_az, cos_az) = self.azimuth.sin_cos();
        let (sin_el, cos_el) = self.elevation.sin_cos();
        self.focus
            + Vec3::new(cos_el * sin_az, sin_el, cos_el * cos_az) * self.radius
    }

    pub fn forward(&self) -> Vec3 {
        (self.focus - self.position()).normalize()
    }

    pub fn right(&self) -> Vec3 {
        Vec3::Y.cross(self.forward()).normalize()
    }

    pub fn up(&self) -> Vec3 {
        self.forward().cross(self.right()).normalize()
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position(), self.focus, Vec3::Y)
    }

    /// Orbit by (d_azimuth, d_elevation) radians. Elevation clamped to ±89°.
    pub fn orbit(&mut self, d_az: f32, d_el: f32) {
        self.azimuth += d_az;
        self.elevation = (self.elevation + d_el)
            .clamp(-std::f32::consts::FRAC_PI_2 + 0.02, std::f32::consts::FRAC_PI_2 - 0.02);
    }

    /// Move focus toward/away from camera (zoom). Radius clamped to [1, 2000].
    pub fn zoom(&mut self, delta: f32) {
        self.radius = (self.radius - delta).clamp(1.0, 2000.0);
    }
}

pub fn projection_matrix(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    Mat4::perspective_rh(fov_y_rad, aspect, near, far)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn default_cam() -> Camera {
        Camera::new(Vec3::new(40.0, 0.0, 20.0), 0.0, 0.0, 120.0)
    }

    #[test]
    fn position_is_offset_from_focus_by_radius() {
        let cam = default_cam();
        let dist = (cam.position() - cam.focus).length();
        assert!((dist - cam.radius).abs() < 1e-4, "radius mismatch: {dist}");
    }

    #[test]
    fn forward_points_at_focus() {
        let cam = default_cam();
        let expected = (cam.focus - cam.position()).normalize();
        let got = cam.forward();
        let dot = expected.dot(got);
        assert!(dot > 0.9999, "forward not toward focus: dot={dot}");
    }

    #[test]
    fn right_is_perpendicular_to_forward() {
        let cam = default_cam();
        let dot = cam.right().dot(cam.forward()).abs();
        assert!(dot < 1e-5, "right not perpendicular to forward: {dot}");
    }

    #[test]
    fn elevation_clamps() {
        let mut cam = default_cam();
        cam.orbit(0.0, 1e6);
        assert!(cam.elevation < PI / 2.0);
    }

    #[test]
    fn view_proj_matrix_is_finite() {
        let cam = default_cam();
        let vp = projection_matrix(std::f32::consts::FRAC_PI_3, 16.0 / 9.0, 0.1, 2000.0)
            * cam.view_matrix();
        for v in vp.to_cols_array() {
            assert!(v.is_finite(), "non-finite in view-proj: {v}");
        }
    }

    #[test]
    fn zoom_clamps_radius() {
        let mut cam = default_cam();
        cam.zoom(1e9);
        assert!(cam.radius >= 1.0);
        cam.zoom(-1e9);
        assert!(cam.radius <= 2000.0);
    }
}
