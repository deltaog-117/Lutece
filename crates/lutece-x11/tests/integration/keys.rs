//! Key bindings: a press on the X server reaches the engine and changes what is shown.

use crate::support::{CTRL, Client, Geometry, SHIFT, SPACE, SUPER, Session, Spec, key};

const MASTER: Geometry = Geometry::new(0, 0, 704, 720);
const SIDE: Geometry = Geometry::new(704, 0, 576, 720);
const FLOATING: Geometry = Geometry::new(256, 144, 768, 432);

#[test]
fn super_f_floats_the_focused_window_and_a_second_press_tiles_it_again() {
    let Some(session) = Session::start() else {
        return;
    };
    let first = session.client.open(Spec::default());
    let second = session.client.open(Spec::default());
    session.client.expect_geometry(second, MASTER);

    session.client.press(&[SUPER], key('f'));
    session.client.expect_geometry(second, FLOATING);
    session.client.expect_geometry(first, Session::screen());

    session.client.press(&[SUPER], key('f'));
    session.client.expect_geometry(second, MASTER);
    session.client.expect_geometry(first, SIDE);
}

#[test]
fn a_floating_window_moves_with_super_ctrl_and_the_direction_keys() {
    let Some(session) = Session::start() else {
        return;
    };
    let window = session.client.open(Spec::default());
    session.client.press(&[SUPER], key('f'));
    session.client.expect_geometry(window, FLOATING);

    session.client.press(&[SUPER, CTRL], key('h'));

    let moved = session.client.wait_for("the window to move left", || {
        session.client.geometry(window).filter(|g| g.x < FLOATING.x)
    });
    assert_eq!(
        (moved.y, moved.width, moved.height),
        (FLOATING.y, FLOATING.width, FLOATING.height)
    );
}

#[test]
fn super_l_widens_the_master_column() {
    let Some(session) = Session::start() else {
        return;
    };
    let first = session.client.open(Spec::default());
    let second = session.client.open(Spec::default());
    session.client.expect_geometry(second, MASTER);

    session.client.press(&[SUPER], key('l'));

    // 60% of 1280 pixels.
    session
        .client
        .expect_geometry(second, Geometry::new(0, 0, 768, 720));
    session
        .client
        .expect_geometry(first, Geometry::new(768, 0, 512, 720));
}

#[test]
fn bindings_follow_their_keys_when_the_keyboard_is_remapped() {
    let Some(mut session) = Session::start() else {
        return;
    };
    let _first = session.client.open(Spec::default());
    let second = session.client.open(Spec::default());
    session.client.expect_geometry(second, MASTER);

    // `l` and `x` trade places: Super+l now needs the key that used to be `x`.
    session.client.swap_keys(key('l'), key('x'));

    // The window manager regrabs when it sees the remap, which may be after the first
    // presses; a press nobody grabbed does nothing, so pressing until it works is safe.
    let widened = session
        .client
        .wait_for("Super+l to grow the master again", || {
            session.client.press(&[SUPER], key('l'));
            session
                .client
                .geometry(second)
                .filter(|g| g.width > MASTER.width)
        });
    assert_eq!(widened.height, MASTER.height);
}

#[test]
fn super_space_switches_to_the_monocle_layout() {
    let Some(session) = Session::start() else {
        return;
    };
    let _first = session.client.open(Spec::default());
    let second = session.client.open(Spec::default());
    session.client.expect_geometry(second, MASTER);

    session.client.press(&[SUPER], SPACE);

    session.client.expect_geometry(second, Session::screen());
}

#[test]
fn a_window_moved_to_another_workspace_is_hidden_until_that_workspace_is_shown() {
    let Some(session) = Session::start() else {
        return;
    };
    let first = session.client.open(Spec::default());
    let second = session.client.open(Spec::default());
    session.client.expect_geometry(second, MASTER);

    session.client.press(&[SUPER, SHIFT], key('2'));
    session.client.expect_geometry(first, Session::screen());
    session
        .client
        .wait_for("the moved window to be hidden", || {
            (!session.client.is_shown(second)).then_some(())
        });

    session.client.press(&[SUPER], key('2'));
    session.client.wait_for("the workspace to be shown", || {
        session.client.is_shown(second).then_some(())
    });
    session.client.expect_geometry(second, Session::screen());
    assert!(!session.client.is_shown(first));
}

#[test]
fn super_shift_c_closes_the_focused_window() {
    let Some(session) = Session::start() else {
        return;
    };
    let first = session.client.open(Spec::default());
    // Its own connection: a window that cannot be asked to close is closed by ending
    // its client, and the observer must survive that.
    let other = Client::connect(session.display());
    let doomed = other.open(Spec::default());
    session.client.expect_geometry(doomed, MASTER);

    session.client.press(&[SUPER, SHIFT], key('c'));

    session.client.wait_for("the window to be closed", || {
        session.client.geometry(doomed).is_none().then_some(())
    });
    session.client.expect_geometry(first, Session::screen());
}

#[test]
fn super_shift_q_quits_the_window_manager() {
    let Some(mut session) = Session::start() else {
        return;
    };

    session.client.press(&[SUPER, SHIFT], key('q'));

    assert!(session.wm_exited());
}
