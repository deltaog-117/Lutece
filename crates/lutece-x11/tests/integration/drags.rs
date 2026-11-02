//! Mouse drags: pointer presses and motion on the X server reach the engine and move or
//! resize a floating window. The example holds Super for drags.

use crate::support::{Geometry, SUPER, Session, Spec, key};

const FLOATING: Geometry = Geometry::new(256, 144, 768, 432);

/// A point inside the floating window, away from its edges.
const GRIP: (i16, i16) = (500, 300);

const LEFT: u8 = 1;
const RIGHT: u8 = 3;

/// A window that has been made floating, with the pointer not yet moved.
fn floating_window() -> Option<(Session, u32)> {
    let session = Session::start()?;
    let window = session.client.open(Spec::default());
    session.client.press(&[SUPER], key('f'));
    session.client.expect_geometry(window, FLOATING);
    Some((session, window))
}

#[test]
fn super_left_drag_moves_a_floating_window_by_the_pointer_distance() {
    let Some((session, window)) = floating_window() else {
        return;
    };

    session.client.drag(&[SUPER], LEFT, GRIP, (540, 330));

    session
        .client
        .expect_geometry(window, Geometry::new(296, 174, 768, 432));
}

#[test]
fn super_right_drag_resizes_a_floating_window_by_the_pointer_distance() {
    let Some((session, window)) = floating_window() else {
        return;
    };

    session.client.drag(&[SUPER], RIGHT, GRIP, (560, 340));

    session
        .client
        .expect_geometry(window, Geometry::new(256, 144, 828, 472));
}

#[test]
fn a_drag_without_the_modifier_leaves_the_window_where_it_is() {
    let Some((session, window)) = floating_window() else {
        return;
    };

    session.client.drag(&[], LEFT, GRIP, (540, 330));

    // Nothing is grabbed, so the click goes to the window and no adjustment is sent. A
    // key press afterwards is handled in order, so the move would have shown by then.
    session.client.press(&[SUPER], key('f'));
    session.client.press(&[SUPER], key('f'));
    session.client.expect_geometry(window, FLOATING);
}
