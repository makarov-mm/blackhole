//! Binary entry point. The renderer needs the `render` feature (winit/glutin/glow):
//!     cargo run --release --features render
//! The headless core checks build without it:
//!     cargo run --release --bin verify

fn main() {
    #[cfg(feature = "render")]
    {
        blackhole::app::run();
    }
    #[cfg(not(feature = "render"))]
    {
        eprintln!(
            "This is the GL viewer binary; build it with the render feature:\n\
             \n    cargo run --release --features render\n\n\
             (the headless validator needs no GL:  cargo run --release --bin verify)"
        );
    }
}
