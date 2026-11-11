//! Holding the keyboard and the pointer back from the programs, for a view of the past.

use crate::support::Session;
use lutece_x11::{Error, X11Backend};

#[test]
fn input_is_held_back_until_it_is_given_back() {
    let Some(session) = Session::bare() else {
        return;
    };
    let mut backend = X11Backend::connect_to(Some(session.display())).expect("becomes the manager");
    assert!(
        session.client.can_take_input(),
        "nothing is held at the start"
    );

    backend
        .set_read_only(true)
        .expect("nobody else holds the input");
    assert!(backend.is_read_only());
    assert!(
        !session.client.can_take_input(),
        "a program cannot take the keyboard or the pointer while they are held"
    );

    backend.set_read_only(false).expect("gives it back");
    assert!(!backend.is_read_only());
    assert!(
        session.client.can_take_input(),
        "the programs have them again"
    );
}

#[test]
fn asking_for_the_state_it_already_has_changes_nothing() {
    let Some(session) = Session::bare() else {
        return;
    };
    let mut backend = X11Backend::connect_to(Some(session.display())).expect("becomes the manager");

    backend.set_read_only(false).expect("already giving it");
    backend.set_read_only(true).expect("holds");
    backend.set_read_only(true).expect("already holding");
    assert!(!session.client.can_take_input());

    backend.set_read_only(false).expect("gives it back");
    assert!(session.client.can_take_input());
}

#[test]
fn input_is_given_back_when_the_manager_goes_away() {
    let Some(session) = Session::bare() else {
        return;
    };
    let mut backend = X11Backend::connect_to(Some(session.display())).expect("becomes the manager");
    backend.set_read_only(true).expect("holds");
    assert!(!session.client.can_take_input());

    // A crashed host must not leave the screen dead: the server frees the grabs with the
    // connection.
    drop(backend);

    assert!(session.client.can_take_input());
}

#[test]
fn a_client_that_holds_the_input_stops_the_hold_and_nothing_stays_taken() {
    let Some(session) = Session::bare() else {
        return;
    };
    let mut backend = X11Backend::connect_to(Some(session.display())).expect("becomes the manager");
    let locker = crate::support::Client::connect(session.display());
    locker.hold_input_until_dropped();

    let refused = backend.set_read_only(true);

    assert!(
        matches!(refused, Err(Error::InputAlreadyGrabbed)),
        "{refused:?}"
    );
    assert!(!backend.is_read_only());
}
