//! Several monitors: each shows its own workspace, and keys move windows and focus
//! between them.

use crate::support::{Geometry, SHIFT, SUPER, Session, Spec, key};
use crate::support::{SCREEN_HEIGHT, SCREEN_WIDTH};

const LEFT: Geometry = Geometry::new(0, 0, SCREEN_WIDTH / 2, SCREEN_HEIGHT);
const RIGHT: Geometry = Geometry::new(
    (SCREEN_WIDTH / 2) as i16,
    0,
    SCREEN_WIDTH / 2,
    SCREEN_HEIGHT,
);

/// A session whose server has two monitors from before the window manager starts, so the
/// first report already lists both.
fn two_monitors() -> Option<Session> {
    let mut session = Session::bare()?;
    session
        .client
        .split_into_two_monitors(SCREEN_WIDTH, SCREEN_HEIGHT);
    session.start_wm();
    Some(session)
}

#[test]
fn a_window_fills_the_monitor_of_the_workspace_it_is_on() {
    let Some(session) = two_monitors() else {
        return;
    };
    let window = session.client.open(Spec::default());

    session.client.expect_geometry(window, LEFT);
}

#[test]
fn super_shift_period_sends_the_focused_window_to_the_other_monitor() {
    let Some(session) = two_monitors() else {
        return;
    };
    let first = session.client.open(Spec::default());
    let second = session.client.open(Spec::default());

    session.client.press(&[SUPER, SHIFT], key('.'));

    session.client.expect_geometry(second, RIGHT);
    session.client.expect_geometry(first, LEFT);
}

#[test]
fn super_period_moves_the_focus_so_new_windows_open_on_the_other_monitor() {
    let Some(session) = two_monitors() else {
        return;
    };
    let first = session.client.open(Spec::default());

    session.client.press(&[SUPER], key('.'));
    let second = session.client.open(Spec::default());

    session.client.expect_geometry(second, RIGHT);
    session.client.expect_geometry(first, LEFT);
}

#[test]
fn a_bar_over_one_monitor_keeps_space_clear_on_that_monitor_only() {
    let Some(session) = two_monitors() else {
        return;
    };
    // The bar covers the left monitor's columns 0..=639, so its strut touches only that one.
    let half = SCREEN_WIDTH / 2;
    session
        .client
        .open(Spec::at(0, 0, half, 30).top_bar_between(30, 0, u32::from(half) - 1));
    let left = session.client.open(Spec::default());
    session.client.press(&[SUPER], key('.'));
    let right = session.client.open(Spec::default());

    session
        .client
        .expect_geometry(left, Geometry::new(0, 30, half, SCREEN_HEIGHT - 30));
    session.client.expect_geometry(right, RIGHT);
}
