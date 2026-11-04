//! Bars: windows of type dock are left where they are, and the windows are tiled in the
//! space their struts leave.

use crate::support::{Geometry, SCREEN_HEIGHT, SCREEN_WIDTH, Session, Spec};

const BAR: Spec = Spec::at(0, 0, SCREEN_WIDTH, 30);

/// The screen below a bar of `depth` pixels along the top.
const fn below(depth: u16) -> Geometry {
    Geometry::new(0, depth as i16, SCREEN_WIDTH, SCREEN_HEIGHT - depth)
}

#[test]
fn a_bar_stays_where_it_is_and_the_tiles_start_below_it() {
    let Some(session) = Session::start() else {
        return;
    };
    let bar = session.client.open(BAR.top_bar(30));
    let window = session.client.open(Spec::default());

    session.client.expect_geometry(window, below(30));
    session
        .client
        .expect_geometry(bar, Geometry::new(0, 0, SCREEN_WIDTH, 30));
    assert!(session.client.is_shown(bar));
}

#[test]
fn a_bar_that_was_showing_before_the_window_manager_started_is_honoured() {
    let Some(mut session) = Session::bare() else {
        return;
    };
    let bar = session.client.open(BAR.top_bar(30));
    session.start_wm();

    let window = session.client.open(Spec::default());
    session.client.expect_geometry(window, below(30));
    // It was left alone, not tiled to the screen.
    session
        .client
        .expect_geometry(bar, Geometry::new(0, 0, SCREEN_WIDTH, 30));
}

#[test]
fn closing_a_bar_gives_its_space_back() {
    let Some(session) = Session::start() else {
        return;
    };
    let bar = session.client.open(BAR.top_bar(30));
    let window = session.client.open(Spec::default());
    session.client.expect_geometry(window, below(30));

    session.client.destroy(bar);

    session.client.expect_geometry(window, Session::screen());
}

#[test]
fn a_bar_that_grows_pushes_the_tiles_further_down() {
    let Some(session) = Session::start() else {
        return;
    };
    let bar = session.client.open(BAR.top_bar(30));
    let window = session.client.open(Spec::default());
    session.client.expect_geometry(window, below(30));

    session
        .client
        .set_strut(bar, [0, 0, 50, 0, 0, 0, 0, 0, 0, u32::MAX, 0, 0]);

    session.client.expect_geometry(window, below(50));
}

#[test]
fn two_bars_on_one_edge_reserve_the_larger_one() {
    let Some(session) = Session::start() else {
        return;
    };
    session.client.open(BAR.top_bar(30));
    session
        .client
        .open(Spec::at(0, 0, SCREEN_WIDTH, 50).top_bar(50));
    let window = session.client.open(Spec::default());

    session.client.expect_geometry(window, below(50));
}

#[test]
fn a_bar_that_manages_itself_still_reserves_its_struts() {
    let Some(session) = Session::start() else {
        return;
    };
    let bar = session.client.open(BAR.top_bar(30).unmanaged());
    let window = session.client.open(Spec::default());

    session.client.expect_geometry(window, below(30));
    // It was never managed, so it stays where it put itself.
    session
        .client
        .expect_geometry(bar, Geometry::new(0, 0, SCREEN_WIDTH, 30));
}

#[test]
fn an_unmanaged_window_without_struts_reserves_nothing() {
    let Some(session) = Session::start() else {
        return;
    };
    session.client.open(Spec::at(100, 100, 200, 80).unmanaged());
    let window = session.client.open(Spec::default());

    session.client.expect_geometry(window, Session::screen());
}

#[test]
fn an_unmanaged_bar_that_sets_its_struts_after_it_maps_is_honoured() {
    let Some(session) = Session::start() else {
        return;
    };
    let bar = session.client.open(BAR.unmanaged());
    let window = session.client.open(Spec::default());
    session.client.expect_geometry(window, Session::screen());

    session
        .client
        .set_strut(bar, [0, 0, 30, 0, 0, 0, 0, 0, 0, u32::MAX, 0, 0]);

    session.client.expect_geometry(window, below(30));
}

#[test]
fn an_unmanaged_bar_that_was_showing_before_the_window_manager_started_is_honoured() {
    let Some(mut session) = Session::bare() else {
        return;
    };
    session.client.open(BAR.top_bar(30).unmanaged());
    session.start_wm();

    let window = session.client.open(Spec::default());
    session.client.expect_geometry(window, below(30));
}

#[test]
fn closing_an_unmanaged_bar_gives_its_space_back() {
    let Some(session) = Session::start() else {
        return;
    };
    let bar = session.client.open(BAR.top_bar(30).unmanaged());
    let window = session.client.open(Spec::default());
    session.client.expect_geometry(window, below(30));

    session.client.destroy(bar);

    session.client.expect_geometry(window, Session::screen());
}

#[test]
fn a_client_cannot_use_a_bar_to_squeeze_the_windows_out() {
    let Some(session) = Session::start() else {
        return;
    };
    session
        .client
        .open(Spec::at(0, 0, SCREEN_WIDTH, 700).top_bar(700));
    let window = session.client.open(Spec::default());

    // More than half of the screen is ignored, so the window still has room.
    session.client.expect_geometry(window, Session::screen());
}
