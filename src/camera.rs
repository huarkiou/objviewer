// Orbit camera controller
// Ported from OrbitCamera3D.cs

use glam::{Mat4, Vec3};

pub struct Camera {
    /// Yaw angle in radians (rotation around Y axis)
    pub yaw: f32,
    /// Pitch angle in radians (rotation around X axis)
    pub pitch: f32,
    /// Distance from pivot point
    pub distance: f32,
    /// Field of view (perspective) or ortho size
    pub fov: f32,
    /// Whether using orthographic projection
    pub orthographic: bool,
    /// Aspect ratio (width / height)
    pub aspect: f32,
    /// Near clipping plane
    pub near: f32,
    /// Far clipping plane
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            distance: 2.0,
            fov: 75.0_f32.to_radians(),
            orthographic: true,
            aspect: 1.0,
            near: 0.01,
            far: 100.0,
        }
    }
}

impl Camera {
    /// Rotate camera by delta angles (radians)
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch =
            (self.pitch + delta_pitch).clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
    }

    /// Zoom by delta (scroll wheel).
    /// For orthographic: adjusts the ortho size (half-height).
    /// For perspective: adjusts the field of view.
    pub fn zoom(&mut self, delta: f32) {
        self.fov *= 1.0 - delta;
        self.fov = self.fov.clamp(0.01, 3.0);
    }

    /// Toggle between perspective and orthographic projection
    pub fn toggle_projection(&mut self) {
        self.orthographic = !self.orthographic;
    }

    /// Reset camera to default view
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Set camera to a specific view direction
    #[allow(dead_code)]
    pub fn set_view(&mut self, yaw: f32, pitch: f32) {
        self.yaw = yaw;
        self.pitch = pitch;
    }

    /// Compute view matrix (camera looking at origin)
    pub fn view_matrix(&self) -> Mat4 {
        let eye = Vec3::new(
            self.distance * self.pitch.cos() * self.yaw.sin(),
            self.distance * self.pitch.sin(),
            self.distance * self.pitch.cos() * self.yaw.cos(),
        );
        Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y)
    }

    /// Compute projection matrix
    ///
    /// NOTE: glam's `perspective_rh` and `orthographic_rh` produce OpenGL-style NDC
    /// with z in [-1, 1]. wgpu expects z in [0, 1] (Vulkan/D3D convention).
    /// A conversion matrix (scale + translate z) will be needed later — fine for now.
    pub fn projection_matrix(&self) -> Mat4 {
        if self.orthographic {
            let half = self.fov;
            Mat4::orthographic_rh(
                -half * self.aspect,
                half * self.aspect,
                -half,
                half,
                self.near,
                self.far,
            )
        } else {
            Mat4::perspective_rh(self.fov, self.aspect, self.near, self.far)
        }
    }

    // ── View presets ──

    /// Look along -Z (front)
    pub fn view_front(&mut self) {
        self.yaw = 0.0;
        self.pitch = 0.0;
    }

    /// Look along +Z (back)
    pub fn view_back(&mut self) {
        self.yaw = std::f32::consts::PI;
        self.pitch = 0.0;
    }

    /// Look along -X (left)
    pub fn view_left(&mut self) {
        self.yaw = -std::f32::consts::FRAC_PI_2;
        self.pitch = 0.0;
    }

    /// Look along +X (right)
    pub fn view_right(&mut self) {
        self.yaw = std::f32::consts::FRAC_PI_2;
        self.pitch = 0.0;
    }

    /// Look along -Y (top-down)
    pub fn view_top(&mut self) {
        self.yaw = 0.0;
        self.pitch = -std::f32::consts::FRAC_PI_2;
    }

    /// Look along +Y (bottom-up)
    pub fn view_bottom(&mut self) {
        self.yaw = 0.0;
        self.pitch = std::f32::consts::FRAC_PI_2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: approx equality for f32
    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn test_default_values() {
        let cam = Camera::default();
        assert!(approx_eq(cam.yaw, 0.0));
        assert!(approx_eq(cam.pitch, 0.0));
        assert!(approx_eq(cam.distance, 2.0));
        assert!(approx_eq(cam.fov, 75.0_f32.to_radians()));
        assert!(cam.orthographic);
        assert!(approx_eq(cam.aspect, 1.0));
        assert!(approx_eq(cam.near, 0.01));
        assert!(approx_eq(cam.far, 100.0));
    }

    #[test]
    fn test_orbit_clamps_pitch() {
        let mut cam = Camera::default();

        // Orbit within limits — should work
        cam.orbit(1.0, 0.5);
        assert!(cam.pitch <= 89.0_f32.to_radians());
        assert!(cam.pitch >= -89.0_f32.to_radians());

        // Orbit far beyond upper limit — should clamp
        cam.pitch = 0.0;
        cam.orbit(0.0, 2.0); // +2 rad ≈ 114°, should clamp to 89°
        assert!(approx_eq(cam.pitch, 89.0_f32.to_radians()));

        // Orbit far beyond lower limit — should clamp
        cam.pitch = 0.0;
        cam.orbit(0.0, -2.0); // -2 rad ≈ -114°, should clamp to -89°
        assert!(approx_eq(cam.pitch, -89.0_f32.to_radians()));
    }

    #[test]
    fn test_view_matrix_default_direction() {
        // Default: yaw=0, pitch=0 → camera looks along -Z
        let cam = Camera::default();
        let view = cam.view_matrix();

        // Transform origin to view space — should be distance units along -Z
        let origin_view = view.transform_point3(glam::Vec3::ZERO);
        // In view space, origin should have negative z (camera looks down -Z)
        assert!(origin_view.z < 0.0);
        // The z component should equal the camera's distance from origin
        assert!(approx_eq(origin_view.z, -cam.distance));
    }

    #[test]
    fn test_view_matrix_front_back() {
        let mut cam = Camera::default();

        // Front (default): eye at (0, 0, +distance), looking at origin
        cam.view_front();
        let view = cam.view_matrix();
        let origin_view = view.transform_point3(glam::Vec3::ZERO);
        assert!(origin_view.z < 0.0);

        // Back: eye at (0, 0, -distance), looking at origin
        cam.view_back();
        let view = cam.view_matrix();
        let origin_view = view.transform_point3(glam::Vec3::ZERO);
        assert!(origin_view.z < 0.0); // origin still in front of camera
    }

    #[test]
    fn test_projection_mode_toggle() {
        let mut cam = Camera::default();
        assert!(cam.orthographic);

        cam.toggle_projection();
        assert!(!cam.orthographic);

        cam.toggle_projection();
        assert!(cam.orthographic);
    }

    #[test]
    fn test_projection_matrix_ortho_vs_perspective() {
        let mut cam = Camera::default();
        cam.orthographic = true;
        let ortho = cam.projection_matrix();

        cam.orthographic = false;
        let persp = cam.projection_matrix();

        // The matrices should differ
        assert!(ortho != persp);
    }

    #[test]
    fn test_view_presets() {
        let mut cam = Camera::default();

        cam.view_front();
        assert!(approx_eq(cam.yaw, 0.0));
        assert!(approx_eq(cam.pitch, 0.0));

        cam.view_back();
        assert!(approx_eq(cam.yaw, std::f32::consts::PI));
        assert!(approx_eq(cam.pitch, 0.0));

        cam.view_left();
        assert!(approx_eq(cam.yaw, -std::f32::consts::FRAC_PI_2));
        assert!(approx_eq(cam.pitch, 0.0));

        cam.view_right();
        assert!(approx_eq(cam.yaw, std::f32::consts::FRAC_PI_2));
        assert!(approx_eq(cam.pitch, 0.0));

        cam.view_top();
        assert!(approx_eq(cam.yaw, 0.0));
        assert!(approx_eq(cam.pitch, -std::f32::consts::FRAC_PI_2));

        cam.view_bottom();
        assert!(approx_eq(cam.yaw, 0.0));
        assert!(approx_eq(cam.pitch, std::f32::consts::FRAC_PI_2));
    }

    #[test]
    fn test_reset() {
        let mut cam = Camera::default();
        cam.yaw = 3.0;
        cam.pitch = 0.5;
        cam.distance = 10.0;
        cam.fov = 45.0_f32.to_radians();
        cam.orthographic = false;
        cam.aspect = 2.0;
        cam.near = 0.1;
        cam.far = 200.0;

        cam.reset();
        let default = Camera::default();
        assert!(approx_eq(cam.yaw, default.yaw));
        assert!(approx_eq(cam.pitch, default.pitch));
        assert!(approx_eq(cam.distance, default.distance));
        assert!(approx_eq(cam.fov, default.fov));
        assert_eq!(cam.orthographic, default.orthographic);
        assert!(approx_eq(cam.aspect, default.aspect));
        assert!(approx_eq(cam.near, default.near));
        assert!(approx_eq(cam.far, default.far));
    }
}
