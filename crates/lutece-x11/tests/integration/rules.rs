//! The example's manage rules: Xmessage is left alone, Firefox goes to workspace 9, and
//! dialogs float.

use crate::support::{Geometry, SUPER, Session, Spec, key};

/// The default floating rectangle: 60% of the screen, centered.
const FLOATING: Geometry = Geometry::new(256, 144, 768, 432);

#[test]
fn an_ignored_window_keeps_its_own_geometry_and_stays_out_of_the_layout() {
    let Some(session) = Session::start() else {
        return;
    };
    let asked = Geometry::new(100, 100, 200, 150);
    let message = session
        .client
        .open(Spec::at(asked.x, asked.y, asked.width, asked.height).class("Xmessage"));
    let tiled = session.client.open(Spec::default());

    // The tiled window has the whole screen, so the ignored one takes no part in the layout.
    session.client.expect_geometry(tiled, Session::screen());
    session.client.expect_geometry(message, asked);
    assert!(session.client.is_shown(message));
}

#[test]
fn a_window_sent_to_another_workspace_appears_when_that_workspace_is_shown() {
    let Some(session) = Session::start() else {
        return;
    };
    let browser = session.client.open(Spec::default().class("Firefox"));
    let tiled = session.client.open(Spec::default());

    // Events are handled in order, so once the second window is placed the first was seen.
    session.client.expect_geometry(tiled, Session::screen());
    assert!(!session.client.is_shown(browser));

    session.client.press(&[SUPER], key('9'));

    session.client.wait_for("the browser to be shown", || {
        session.client.is_shown(browser).then_some(())
    });
    session.client.expect_geometry(browser, Session::screen());
    assert!(!session.client.is_shown(tiled));
}

#[test]
fn a_dialog_opens_floating_over_its_parent() {
    let Some(session) = Session::start() else {
        return;
    };
    let parent = session.client.open(Spec::default());
    let dialog = session.client.open(Spec::default().transient_for(parent));

    session.client.expect_geometry(dialog, FLOATING);
    session.client.expect_geometry(parent, Session::screen());
}
