//! Windows opening, closing and being adopted are laid out by the tile layout.

use crate::support::{Geometry, Session, Spec};

/// The master column at the default 55% of a 1280-pixel-wide screen.
const MASTER: Geometry = Geometry::new(0, 0, 704, 720);
const SIDE: Geometry = Geometry::new(704, 0, 576, 720);

#[test]
fn a_single_window_fills_the_screen() {
    let Some(session) = Session::start() else {
        return;
    };
    let window = session.client.open(Spec::default());

    session.client.expect_geometry(window, Session::screen());
}

#[test]
fn the_newest_window_takes_the_master_column() {
    let Some(session) = Session::start() else {
        return;
    };
    let first = session.client.open(Spec::default());
    let second = session.client.open(Spec::default());

    session.client.expect_geometry(second, MASTER);
    session.client.expect_geometry(first, SIDE);
}

#[test]
fn closing_a_window_gives_its_space_back() {
    let Some(session) = Session::start() else {
        return;
    };
    let first = session.client.open(Spec::default());
    let second = session.client.open(Spec::default());
    session.client.expect_geometry(second, MASTER);

    session.client.destroy(second);

    session.client.expect_geometry(first, Session::screen());
}

#[test]
fn windows_open_before_the_window_manager_starts_are_adopted() {
    let Some(mut session) = Session::bare() else {
        return;
    };
    let first = session.client.open(Spec::at(30, 40, 200, 100));
    let second = session.client.open(Spec::at(60, 80, 200, 100));

    session.start_wm();

    session.client.expect_geometry(second, MASTER);
    session.client.expect_geometry(first, SIDE);
}
