//! Shows the colour wheel on the first connected screen for a few seconds.
//!
//! Run it from a virtual terminal (switch with Ctrl+Alt+F3), not from inside a graphical
//! session: `cargo run -p lutece-wayland --features drm --example drm_pattern -- 300`.
//! The argument is the number of frames; at 60 Hz, 300 is five seconds.

use lutece_wayland::show_test_pattern;

fn main() {
    let frames = std::env::args()
        .nth(1)
        .and_then(|text| text.parse().ok())
        .unwrap_or(300);
    match show_test_pattern(frames) {
        Ok(report) => println!(
            "showed {} frames on {} at {}x{} {} Hz",
            report.frames, report.connector, report.width, report.height, report.refresh_hz
        ),
        Err(error) => {
            eprintln!("drm_pattern: {error}");
            std::process::exit(1);
        }
    }
}
