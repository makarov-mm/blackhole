//! Headless validation of the black-hole renderer core (no GL):
//!   * weak-field light deflection vs. General Relativity (2 r_s / b),
//!   * a rendered frame written to a PPM so the lensing/disk can be eyeballed.

use blackhole::blackhole::{deflection, trace, RS};
use blackhole::camera::OrbitCamera;
use blackhole::math::{vec3, Vec3};
use std::io::Write;

fn tonemap(c: Vec3) -> (u8, u8, u8) {
    let m = |x: f32| {
        let t = x / (x + 1.0); // Reinhard
        let g = t.max(0.0).powf(1.0 / 2.2); // gamma
        (g.clamp(0.0, 1.0) * 255.0) as u8
    };
    (m(c.x), m(c.y), m(c.z))
}

fn main() {
    // --- 1. light deflection vs General Relativity -----------------------
    println!("=== light deflection vs GR (r_s = 1) ===");
    println!("   impact b   measured     2*r_s/b (Einstein)   ratio");
    for &b in &[6.0f32, 8.0, 12.0, 20.0, 30.0] {
        let meas = deflection(b);
        let gr = 2.0 * RS / b;
        println!("   {:6.1}    {:8.4}     {:8.4}             {:.3}", b, meas, gr, meas / gr);
    }
    println!("   (ratio -> 1 for large b confirms the geodesic constant is correct)\n");

    // photon capture: rays with b below the critical impact parameter
    // (b_crit = 3*sqrt(3)/2 * r_s ~= 2.598) must be swallowed.
    let bc = 3.0f32.sqrt() * 3.0 / 2.0;
    let captured = deflection(2.0).is_infinite();
    let escaped = !deflection(5.0).is_infinite();
    println!("=== photon capture ===");
    println!("   critical impact parameter b_crit = {:.3} r_s", bc);
    println!("   b=2.0 captured: {}   b=5.0 escapes: {}\n", captured, escaped);

    // --- 2. render a frame ----------------------------------------------
    let (w, h) = (600usize, 380usize);
    let mut cam = OrbitCamera::new(22.0, w as f32 / h as f32);
    cam.yaw = 0.9;
    cam.pitch = 0.16; // just above the disk plane -> lensed far side arcs overhead
    let rb = cam.basis();
    let time = 2.0f32;

    let mut img = vec![0u8; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let ndc_x = 2.0 * (x as f32 + 0.5) / w as f32 - 1.0;
            let ndc_y = 1.0 - 2.0 * (y as f32 + 0.5) / h as f32;
            let dir = (rb.forward
                + rb.right * (ndc_x * rb.tan_half * rb.aspect)
                + rb.up * (ndc_y * rb.tan_half))
                .normalized();
            let col = trace(rb.eye, dir, time);
            let (r, g, b) = tonemap(col);
            let o = (y * w + x) * 3;
            img[o] = r;
            img[o + 1] = g;
            img[o + 2] = b;
        }
    }

    let path = std::env::args().nth(1).unwrap_or_else(|| "/tmp/bh.ppm".to_string());
    let mut f = std::fs::File::create(&path).unwrap();
    write!(f, "P6\n{} {}\n255\n", w, h).unwrap();
    f.write_all(&img).unwrap();
    println!("=== render ===");
    println!("   wrote {}x{} frame to {}", w, h, path);
    let _ = vec3(0.0, 0.0, 0.0); // keep vec3 import used if trace inlines
}
