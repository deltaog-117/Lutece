//! Fullscreen: a window that asks, or is told by a key, covers the whole screen, bars
//! included, and goes back when it stops.

use crate::support::{Geometry, SCREEN_HEIGHT, SCREEN_WIDTH, SHIFT, SUPER, Session, Spec, key};

const BAR: Spec = Spec::at(0, 0, SCREEN_WIDTH, 30);
const BELOW_THE_BAR: Geometry = Geometry::new(0, 30, SCREEN_WIDTH, SCREEN_HEIGHT - 30);

const ADD: u32 = 1;
const REMOVE: u32 = 0;
const TOGGLE: u32 = 2;

#[test]
fn the_window_manager_says_it_supports_fullscreen_and_bars() {
    let Some(session) = Session::start() else {
        return;
    };
    for hint in [
        "_NET_WM_STATE",
        "_NET_WM_STATE_FULLSCREEN",
        "_NET_WM_WINDOW_TYPE_DOCK",
        "_NET_WM_STRUT_PARTIAL",
    ] {
        assert!(session.client.supported(hint), "{hint} is not listed");
    }
}

#[test]
fn a_fullscreen_request_covers_the_bar_and_removing_it_puts_the_window_back() {
    let Some(session) = Session::start() else {
        return;
    };
    session.client.open(BAR.top_bar(30));
    let window = session.client.open(Spec::default());
    session.client.expect_geometry(window, BELOW_THE_BAR);

    session.client.request_fullscreen(window, ADD);
    session.client.expect_geometry(window, Session::screen());
    session.client.wait_for("the fullscreen state", || {
        session.client.is_fullscreen(window).then_some(())
    });

    session.client.request_fullscreen(window, REMOVE);
    session.client.expect_geometry(window, BELOW_THE_BAR);
    session.client.wait_for("the state to go", || {
        (!session.client.is_fullscreen(window)).then_some(())
    });
}

#[test]
fn a_toggle_request_switches_the_state_by_what_was_shown() {
    let Some(session) = Session::start() else {
        return;
    };
    session.client.open(BAR.top_bar(30));
    let window = session.client.open(Spec::default());
    session.client.expect_geometry(window, BELOW_THE_BAR);

    session.client.request_fullscreen(window, TOGGLE);
    session.client.expect_geometry(window, Session::screen());

    session.client.request_fullscreen(window, TOGGLE);
    session.client.expect_geometry(window, BELOW_THE_BAR);
}

#[test]
fn the_fullscreen_key_toggles_the_focused_window() {
    let Some(session) = Session::start() else {
        return;
    };
    session.client.open(BAR.top_bar(30));
    let window = session.client.open(Spec::default());
    session.client.expect_geometry(window, BELOW_THE_BAR);

    session.client.press(&[SUPER, SHIFT], key('f'));
    session.client.expect_geometry(window, Session::screen());

    session.client.press(&[SUPER, SHIFT], key('f'));
    session.client.expect_geometry(window, BELOW_THE_BAR);
}

#[test]
fn a_window_that_starts_fullscreen_covers_the_screen_at_once() {
    let Some(session) = Session::start() else {
        return;
    };
    session.client.open(BAR.top_bar(30));
    let window = session.client.open(Spec::default().fullscreen());

    session.client.expect_geometry(window, Session::screen());
}

#[test]
fn a_new_window_ends_the_fullscreen_of_the_one_before() {
    let Some(session) = Session::start() else {
        return;
    };
    let video = session.client.open(Spec::default());
    session.client.request_fullscreen(video, ADD);
    session.client.expect_geometry(video, Session::screen());

    // The window that has the keyboard must not hide behind a fullscreen one.
    let dialog = session.client.open(Spec::default());

    session.client.wait_for("the video to be tiled", || {
        session
            .client
            .geometry(video)
            .filter(|g| *g != Session::screen())
    });
    assert!(!session.client.is_fullscreen(video));
    assert!(session.client.is_shown(dialog));
}

#[test]
fn a_fullscreen_window_keeps_its_state_across_a_workspace_switch() {
    let Some(session) = Session::start() else {
        return;
    };
    let window = session.client.open(Spec::default());
    session.client.request_fullscreen(window, ADD);
    session.client.expect_geometry(window, Session::screen());

    session.client.press(&[SUPER], key('2'));
    session.client.wait_for("the window to be hidden", || {
        (!session.client.is_shown(window)).then_some(())
    });
    session.client.press(&[SUPER], key('1'));

    session.client.expect_geometry(window, Session::screen());
    session.client.wait_for("the fullscreen state", || {
        session.client.is_fullscreen(window).then_some(())
    });
}
