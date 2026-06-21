//! Schwarzschild black-hole renderer (CPU reference).
//!
//! Units: Schwarzschild radius r_s = 1, so the event horizon is at r = 1, the
//! photon sphere at r = 1.5 and the marginally stable orbit (ISCO, disk inner
//! edge) at r = 3. The GLSL fragment shader in `render.rs` mirrors this code
//! one-to-one; this version exists so the maths can be validated headless.
//!
//! Light bending uses the standard reduction of the null geodesic to a central
//! acceleration  a = -1.5 * h^2 * r / |r|^5  (with r_s = 1), where h^2 is the
//! squared specific angular momentum |r x v|^2, conserved along the ray.

use crate::math::{vec3, Vec3};

pub const RS: f32 = 1.0;
pub const DISK_IN: f32 = 3.0;
pub const DISK_OUT: f32 = 11.0;
pub const ESCAPE: f32 = 60.0;
pub const MAX_STEPS: i32 = 600;

#[inline]
fn fract(x: f32) -> f32 {
    x - x.floor()
}
#[inline]
fn clamp(x: f32, a: f32, b: f32) -> f32 {
    x.max(a).min(b)
}
#[inline]
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
#[inline]
fn mix3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}
#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// --- hash / value noise (identical form to the GLSL) ---
#[inline]
fn hash21(x: f32, y: f32) -> f32 {
    fract((x * 127.1 + y * 311.7).sin() * 43758.5453)
}
fn noise2(x: f32, y: f32) -> f32 {
    let ix = x.floor();
    let iy = y.floor();
    let fx = x - ix;
    let fy = y - iy;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let a = hash21(ix, iy);
    let b = hash21(ix + 1.0, iy);
    let c = hash21(ix, iy + 1.0);
    let d = hash21(ix + 1.0, iy + 1.0);
    mix(mix(a, b, ux), mix(c, d, ux), uy)
}
fn fbm2(mut x: f32, mut y: f32) -> f32 {
    let mut v = 0.0;
    let mut amp = 0.5;
    for _ in 0..4 {
        v += amp * noise2(x, y);
        x *= 2.0;
        y *= 2.0;
        amp *= 0.5;
    }
    v
}

#[inline]
fn hash31(c: Vec3) -> f32 {
    fract((c.x * 127.1 + c.y * 311.7 + c.z * 74.7).sin() * 43758.5453)
}

/// Starfield + faint nebula along the (possibly bent) escape direction.
fn background(dir: Vec3) -> Vec3 {
    // nebula: very faint vertical tint
    let t = dir.y * 0.5 + 0.5;
    let mut col = mix3(vec3(0.010, 0.011, 0.020), vec3(0.020, 0.018, 0.035), t);

    // stars: one candidate per lattice cell of dir*scale
    for &scale in &[40.0f32, 75.0] {
        let p = dir * scale;
        let cell = vec3(p.x.floor(), p.y.floor(), p.z.floor());
        let f = p - cell;
        let cx = hash31(cell);
        let cy = hash31(cell + vec3(13.0, 7.0, 19.0));
        let cz = hash31(cell + vec3(5.0, 23.0, 11.0));
        let center = vec3(cx, cy, cz);
        let d = (f - center).length();
        let bright = hash31(cell + vec3(2.0, 2.0, 2.0));
        if bright > 0.86 {
            let s = smoothstep(0.14, 0.0, d) * (bright - 0.86) / 0.14;
            let tint = mix3(vec3(0.7, 0.8, 1.0), vec3(1.0, 0.9, 0.8), hash31(cell + vec3(9.0, 9.0, 9.0)));
            col += tint * (s * 0.9);
        }
    }
    col
}

/// Emission + opacity of the accretion disk at an equatorial-plane hit point.
fn disk_sample(hit: Vec3, eye: Vec3, time: f32) -> (Vec3, f32) {
    let r = hit.length();
    if r < DISK_IN || r > DISK_OUT {
        return (Vec3::default(), 0.0);
    }
    let t = (r - DISK_IN) / (DISK_OUT - DISK_IN); // 0 inner .. 1 outer
    let ang = hit.z.atan2(hit.x);

    // Keplerian rotation: inner radii sweep faster.
    let omega = r.powf(-1.5);
    let swirl = ang * 2.0 - time * omega * 2.4;

    // turbulent density: spiral-ish bands + finer fbm
    let bands = 0.5 + 0.5 * (swirl * 3.0).sin();
    let n = fbm2(swirl * 1.6 + 10.0, r * 0.9);
    let mut density = (0.35 + 0.65 * n) * mix(bands, 1.0, 0.5);

    // radial profile: bright inside, fades to the rim
    density *= (1.0 - smoothstep(0.0, 1.0, t)).powf(0.6) + 0.08;
    density *= smoothstep(0.0, 0.10, t); // soft inner cut at ISCO

    // temperature colour: blue-white inner -> yellow -> orange-red rim
    let inner = vec3(0.75, 0.85, 1.05);
    let midc = vec3(1.0, 0.86, 0.55);
    let outer = vec3(1.0, 0.42, 0.16);
    let mut col = mix3(inner, midc, smoothstep(0.0, 0.45, t));
    col = mix3(col, outer, smoothstep(0.45, 1.0, t));

    // gravitational redshift (dims & reddens near the hole)
    let grav = (1.0 - RS / r).max(0.0).sqrt();

    // relativistic Doppler from Keplerian orbital motion
    let radial = vec3(hit.x, 0.0, hit.z).normalized();
    let orb_dir = vec3(0.0, 1.0, 0.0).cross(radial).normalized(); // rotation direction
    let beta = clamp((0.5 / r).sqrt(), 0.0, 0.85); // v in units of c  (M = 0.5)
    let gamma = 1.0 / (1.0 - beta * beta).sqrt();
    let to_cam = (eye - hit).normalized();
    let mu = orb_dir.dot(to_cam);
    let doppler = 1.0 / (gamma * (1.0 - beta * mu)); // relativistic factor delta

    // beaming: approaching side much brighter
    let beaming = doppler.powi(3);
    // colour shift: approaching -> bluer, receding -> redder
    let sh = clamp(doppler, 0.55, 1.9);
    let shift = vec3(1.0 / sh, 1.0, sh);

    let emit = col * shift * (density * grav * beaming * 1.4);
    let alpha = clamp(density * 1.6, 0.0, 1.0) * 0.92;
    (emit, alpha)
}

/// Trace one photon backwards from the eye. Returns linear RGB.
pub fn trace(eye: Vec3, dir: Vec3, time: f32) -> Vec3 {
    let mut p = eye;
    let mut v = dir.normalized();
    let cr = p.cross(v);
    let h2 = cr.length_sq(); // conserved angular momentum^2

    let mut col = Vec3::default();
    let mut transmittance = 1.0f32;

    for _ in 0..MAX_STEPS {
        let r = p.length();
        if r < RS {
            return col; // captured by the horizon (black)
        }
        if r > ESCAPE {
            col += background(v) * transmittance;
            return col;
        }

        let dt = clamp(r * 0.09, 0.012, 0.5);
        let acc = p * (-1.5 * h2 / r.powi(5));
        let prev = p;
        v += acc * dt;
        p += v * dt;

        // disk lies in the equatorial plane y = 0: detect a sign change
        if prev.y.signum() != p.y.signum() {
            let denom = prev.y - p.y;
            let s = if denom.abs() > 1e-9 { prev.y / denom } else { 0.0 };
            let hit = prev + (p - prev) * s;
            let (emit, alpha) = disk_sample(hit, eye, time);
            col += emit * transmittance;
            transmittance *= 1.0 - alpha;
            if transmittance < 0.02 {
                return col;
            }
        }
    }
    col
}

/// Total light-bending angle (radians) for a ray that starts far away on the
/// -x axis aimed in +x with impact parameter `b` along +y. Used by the verify
/// tool to confirm the weak-field deflection matches General Relativity
/// (Einstein: 2 * r_s / b for large b).
pub fn deflection(b: f32) -> f32 {
    let start = vec3(-40.0, b, 0.0);
    let mut p = start;
    let mut v = vec3(1.0, 0.0, 0.0);
    let h2 = p.cross(v).length_sq();
    for _ in 0..6000 {
        let r = p.length();
        if r < RS {
            return f32::INFINITY; // captured
        }
        if r > 80.0 && v.x > 0.0 {
            break;
        }
        let dt = clamp(r * 0.02, 0.003, 0.1);
        let acc = p * (-1.5 * h2 / r.powi(5));
        v += acc * dt;
        p += v * dt;
    }
    // angle between final direction and the original +x
    let vn = v.normalized();
    vn.y.atan2(vn.x).abs()
}
