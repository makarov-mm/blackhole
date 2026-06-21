//! Orbit camera. The raymarcher needs the eye position and a ray basis
//! (forward / right / up + tan(fov/2) + aspect) rather than a view-projection
//! matrix, so that's what this exposes.

use crate::math::{vec3, Vec3};

pub struct OrbitCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32, // radians (vertical)
    pub aspect: f32,
}

pub struct RayBasis {
    pub eye: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    pub tan_half: f32,
    pub aspect: f32,
}

impl OrbitCamera {
    pub fn new(distance: f32, aspect: f32) -> OrbitCamera {
        OrbitCamera {
            target: Vec3::default(),
            distance,
            yaw: 0.7,
            pitch: 0.22,
            fov: 50.0 * std::f32::consts::PI / 180.0,
            aspect,
        }
    }

    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect;
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * 0.005;
        self.pitch += dy * 0.005;
        let lim = 1.55;
        self.pitch = self.pitch.clamp(-lim, lim);
    }

    pub fn zoom(&mut self, amount: f32) {
        self.distance *= (1.0 - amount * 0.1).clamp(0.5, 1.5);
        self.distance = self.distance.clamp(3.0, 200.0);
    }

    pub fn eye(&self) -> Vec3 {
        let cp = self.pitch.cos();
        let dir = vec3(cp * self.yaw.cos(), self.pitch.sin(), cp * self.yaw.sin());
        self.target + dir * self.distance
    }

    pub fn basis(&self) -> RayBasis {
        let eye = self.eye();
        let forward = (self.target - eye).normalized();
        let world_up = vec3(0.0, 1.0, 0.0);
        let right = forward.cross(world_up).normalized();
        let up = right.cross(forward);
        RayBasis {
            eye,
            forward,
            right,
            up,
            tan_half: (self.fov * 0.5).tan(),
            aspect: self.aspect,
        }
    }
}
