//! Integration tests: the example window manager runs against a real, headless X server
//! and the tests act as its clients, opening windows and pressing keys.
//!
//! They need `Xvfb`. Without it each test prints a notice and passes, so a machine
//! without the server still gets a green check; set `LUTECE_REQUIRE_X11=1` (as CI
//! should) to make a missing server a failure instead.

mod bars;
mod drags;
mod fullscreen;
mod keys;
mod monitors;
mod rules;
mod support;
mod tiling;
