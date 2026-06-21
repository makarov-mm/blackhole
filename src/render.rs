//! GL renderer (glow). The whole image is one fullscreen-triangle pass whose
//! fragment shader ray-marches photon geodesics around a Schwarzschild black
//! hole. The shader is a line-by-line mirror of `crate::blackhole`, which is
//! validated headless (deflection vs GR, photon capture, reference render).

use crate::camera::RayBasis;
use glow::HasContext;

const VS: &str = r#"#version 330 core
out vec2 vNdc;
void main() {
    vec2 p = vec2((gl_VertexID == 1) ? 3.0 : -1.0,
                  (gl_VertexID == 2) ? 3.0 : -1.0);
    vNdc = p;
    gl_Position = vec4(p, 0.0, 1.0);
}
"#;

const FS: &str = r#"#version 330 core
in vec2 vNdc;
out vec4 frag;

uniform vec3  uEye, uForward, uRight, uUp;
uniform float uTanHalf, uAspect, uTime;
uniform int   uSteps;

const float RS       = 1.0;
const float DISK_IN  = 3.0;
const float DISK_OUT = 11.0;
const float ESCAPE   = 60.0;

float hash21(float x, float y) {
    return fract(sin(x * 127.1 + y * 311.7) * 43758.5453);
}
float noise2(float x, float y) {
    float ix = floor(x), iy = floor(y);
    float fx = x - ix,  fy = y - iy;
    float ux = fx * fx * (3.0 - 2.0 * fx);
    float uy = fy * fy * (3.0 - 2.0 * fy);
    float a = hash21(ix, iy);
    float b = hash21(ix + 1.0, iy);
    float c = hash21(ix, iy + 1.0);
    float d = hash21(ix + 1.0, iy + 1.0);
    return mix(mix(a, b, ux), mix(c, d, ux), uy);
}
float fbm2(float x, float y) {
    float v = 0.0, amp = 0.5;
    for (int i = 0; i < 4; ++i) { v += amp * noise2(x, y); x *= 2.0; y *= 2.0; amp *= 0.5; }
    return v;
}
float hash31(vec3 c) {
    return fract(sin(c.x * 127.1 + c.y * 311.7 + c.z * 74.7) * 43758.5453);
}

vec3 background(vec3 dir) {
    float t = dir.y * 0.5 + 0.5;
    vec3 col = mix(vec3(0.010, 0.011, 0.020), vec3(0.020, 0.018, 0.035), t);
    float scales[2] = float[](40.0, 75.0);
    for (int k = 0; k < 2; ++k) {
        vec3 p = dir * scales[k];
        vec3 cell = floor(p);
        vec3 f = p - cell;
        vec3 center = vec3(hash31(cell),
                           hash31(cell + vec3(13.0, 7.0, 19.0)),
                           hash31(cell + vec3(5.0, 23.0, 11.0)));
        float d = length(f - center);
        float bright = hash31(cell + vec3(2.0));
        if (bright > 0.86) {
            float s = smoothstep(0.14, 0.0, d) * (bright - 0.86) / 0.14;
            vec3 tint = mix(vec3(0.7, 0.8, 1.0), vec3(1.0, 0.9, 0.8), hash31(cell + vec3(9.0)));
            col += tint * (s * 0.9);
        }
    }
    return col;
}

// returns emission in .rgb and opacity in .a
vec4 diskSample(vec3 hit, vec3 eye, float time) {
    float r = length(hit);
    if (r < DISK_IN || r > DISK_OUT) return vec4(0.0);
    float t = (r - DISK_IN) / (DISK_OUT - DISK_IN);
    float ang = atan(hit.z, hit.x);

    float omega = pow(r, -1.5);
    float swirl = ang * 2.0 - time * omega * 2.4;

    float bands = 0.5 + 0.5 * sin(swirl * 3.0);
    float n = fbm2(swirl * 1.6 + 10.0, r * 0.9);
    float density = (0.35 + 0.65 * n) * mix(bands, 1.0, 0.5);
    density *= pow(1.0 - smoothstep(0.0, 1.0, t), 0.6) + 0.08;
    density *= smoothstep(0.0, 0.10, t);

    vec3 inner = vec3(0.75, 0.85, 1.05);
    vec3 midc  = vec3(1.0, 0.86, 0.55);
    vec3 outer = vec3(1.0, 0.42, 0.16);
    vec3 col = mix(inner, midc, smoothstep(0.0, 0.45, t));
    col = mix(col, outer, smoothstep(0.45, 1.0, t));

    float grav = sqrt(max(1.0 - RS / r, 0.0));

    vec3 radial = normalize(vec3(hit.x, 0.0, hit.z));
    vec3 orbDir = normalize(cross(vec3(0.0, 1.0, 0.0), radial));
    float beta = clamp(sqrt(0.5 / r), 0.0, 0.85);
    float gamma = 1.0 / sqrt(1.0 - beta * beta);
    vec3 toCam = normalize(eye - hit);
    float mu = dot(orbDir, toCam);
    float doppler = 1.0 / (gamma * (1.0 - beta * mu));

    float beaming = pow(doppler, 3.0);
    float sh = clamp(doppler, 0.55, 1.9);
    vec3 shift = vec3(1.0 / sh, 1.0, sh);

    vec3 emit = col * shift * (density * grav * beaming * 1.4);
    float alpha = clamp(density * 1.6, 0.0, 1.0) * 0.92;
    return vec4(emit, alpha);
}

vec3 trace(vec3 eye, vec3 dir, float time) {
    vec3 p = eye;
    vec3 v = normalize(dir);
    float h2 = dot(cross(p, v), cross(p, v));

    vec3 col = vec3(0.0);
    float transmittance = 1.0;

    for (int i = 0; i < uSteps; ++i) {
        float r = length(p);
        if (r < RS) return col;                 // captured
        if (r > ESCAPE) { col += background(v) * transmittance; return col; }

        float dt = clamp(r * 0.09, 0.012, 0.5);
        vec3 acc = p * (-1.5 * h2 / pow(r, 5.0));
        vec3 prev = p;
        v += acc * dt;
        p += v * dt;

        if (sign(prev.y) != sign(p.y)) {
            float denom = prev.y - p.y;
            float s = abs(denom) > 1e-9 ? prev.y / denom : 0.0;
            vec3 hit = prev + (p - prev) * s;
            vec4 ds = diskSample(hit, eye, time);
            col += ds.rgb * transmittance;
            transmittance *= 1.0 - ds.a;
            if (transmittance < 0.02) return col;
        }
    }
    return col;
}

void main() {
    vec3 dir = normalize(uForward
        + vNdc.x * uTanHalf * uAspect * uRight
        + vNdc.y * uTanHalf * uUp);
    vec3 c = trace(uEye, dir, uTime);
    c = c / (c + vec3(1.0));            // Reinhard tonemap
    c = pow(c, vec3(1.0 / 2.2));        // gamma
    frag = vec4(c, 1.0);
}
"#;

pub struct Renderer {
    program: glow::Program,
    vao: glow::VertexArray,
    u_eye: Option<glow::UniformLocation>,
    u_forward: Option<glow::UniformLocation>,
    u_right: Option<glow::UniformLocation>,
    u_up: Option<glow::UniformLocation>,
    u_tan_half: Option<glow::UniformLocation>,
    u_aspect: Option<glow::UniformLocation>,
    u_time: Option<glow::UniformLocation>,
    u_steps: Option<glow::UniformLocation>,
    pub steps: i32,
}

impl Renderer {
    pub fn new(gl: &glow::Context) -> Renderer {
        unsafe {
            let program = link_program(gl, VS, FS);
            let vao = gl.create_vertex_array().expect("vao"); // empty VAO for the fullscreen triangle
            let u = |n: &str| gl.get_uniform_location(program, n);
            gl.disable(glow::DEPTH_TEST);
            Renderer {
                u_eye: u("uEye"),
                u_forward: u("uForward"),
                u_right: u("uRight"),
                u_up: u("uUp"),
                u_tan_half: u("uTanHalf"),
                u_aspect: u("uAspect"),
                u_time: u("uTime"),
                u_steps: u("uSteps"),
                program,
                vao,
                steps: 400,
            }
        }
    }

    pub fn resize(&self, gl: &glow::Context, w: i32, h: i32) {
        unsafe { gl.viewport(0, 0, w, h) }
    }

    pub fn draw(&self, gl: &glow::Context, rb: &RayBasis, time: f32) {
        unsafe {
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
            gl.use_program(Some(self.program));
            gl.uniform_3_f32(self.u_eye.as_ref(), rb.eye.x, rb.eye.y, rb.eye.z);
            gl.uniform_3_f32(self.u_forward.as_ref(), rb.forward.x, rb.forward.y, rb.forward.z);
            gl.uniform_3_f32(self.u_right.as_ref(), rb.right.x, rb.right.y, rb.right.z);
            gl.uniform_3_f32(self.u_up.as_ref(), rb.up.x, rb.up.y, rb.up.z);
            gl.uniform_1_f32(self.u_tan_half.as_ref(), rb.tan_half);
            gl.uniform_1_f32(self.u_aspect.as_ref(), rb.aspect);
            gl.uniform_1_f32(self.u_time.as_ref(), time);
            gl.uniform_1_i32(self.u_steps.as_ref(), self.steps);
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);
        }
    }
}

unsafe fn link_program(gl: &glow::Context, vs_src: &str, fs_src: &str) -> glow::Program {
    let program = gl.create_program().expect("program");
    let shaders = [(glow::VERTEX_SHADER, vs_src), (glow::FRAGMENT_SHADER, fs_src)];
    let mut handles = Vec::new();
    for (kind, src) in shaders {
        let sh = gl.create_shader(kind).expect("shader");
        gl.shader_source(sh, src);
        gl.compile_shader(sh);
        if !gl.get_shader_compile_status(sh) {
            panic!("shader compile error: {}", gl.get_shader_info_log(sh));
        }
        gl.attach_shader(program, sh);
        handles.push(sh);
    }
    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        panic!("program link error: {}", gl.get_program_info_log(program));
    }
    for sh in handles {
        gl.detach_shader(program, sh);
        gl.delete_shader(sh);
    }
    program
}
