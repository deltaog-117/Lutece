//! The workspace list for bars (`ext_workspace_manager_v1`): what a client is told about the
//! workspaces, and what it may ask for.

use crate::support::{TestClient, backend};
use lutece::{Action, Event};
use lutece_wayland::{WaylandBackend, WorkspaceInfo};

/// `ext_workspace_handle_v1.state.active`.
const ACTIVE: u32 = 1;
/// `ext_workspace_handle_v1.workspace_capabilities.activate`.
const CAN_ACTIVATE: u32 = 1;

fn info(tag: &str, active: bool) -> WorkspaceInfo {
    WorkspaceInfo {
        tag: tag.to_owned(),
        active,
    }
}

fn setup() -> (WaylandBackend, TestClient) {
    let mut backend = backend();
    let (client, _) = TestClient::connect(&mut backend);
    (backend, client)
}

fn view_requests(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::ActionRequested(Action::ViewTag(tag)) => Some(tag.as_str().to_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_compositor_offers_the_workspace_list() {
    let (_, client) = setup();
    assert!(client.has_workspace_list());
}

#[test]
fn each_workspace_is_listed_with_its_name_state_and_what_may_be_asked() {
    let (mut backend, mut client) = setup();
    backend.set_workspaces(&[info("1", true), info("2", false)]);
    client.settle(&mut backend);

    let listed = client.workspaces();
    assert_eq!(listed.len(), 2);
    assert_eq!((listed[0].id.as_str(), listed[0].name.as_str()), ("1", "1"));
    assert_eq!(listed[0].state, ACTIVE);
    assert_eq!(listed[1].state, 0);
    assert!(listed.iter().all(|w| w.capabilities == CAN_ACTIVATE));
    assert!(
        client.workspace_dones() >= 1,
        "the batch is made to take effect"
    );
}

#[test]
fn the_group_holds_every_workspace_and_every_output() {
    let (mut backend, mut client) = setup();
    backend.set_workspaces(&[info("1", true), info("2", false), info("3", false)]);
    client.settle(&mut backend);

    let group = client.workspace_group();
    assert_eq!(group.members, 3);
    assert_eq!(group.outputs, client.output_count());
    assert_eq!(group.capabilities, 0, "no workspace can be created");
}

#[test]
fn a_change_of_the_active_workspace_is_sent_and_no_change_is_not() {
    let (mut backend, mut client) = setup();
    backend.set_workspaces(&[info("1", true), info("2", false)]);
    client.settle(&mut backend);
    let before = client.workspace_dones();

    backend.set_workspaces(&[info("1", true), info("2", false)]);
    client.settle(&mut backend);
    assert_eq!(
        client.workspace_dones(),
        before,
        "nothing changed, nothing sent"
    );

    backend.set_workspaces(&[info("1", false), info("2", true)]);
    client.settle(&mut backend);
    assert_eq!(client.workspaces()[0].state, 0);
    assert_eq!(client.workspaces()[1].state, ACTIVE);
    assert_eq!(client.workspace_dones(), before + 1, "one batch for both");
}

#[test]
fn a_workspace_that_appears_is_listed_and_one_that_goes_is_removed() {
    let (mut backend, mut client) = setup();
    backend.set_workspaces(&[info("1", true), info("2", false)]);
    client.settle(&mut backend);

    backend.set_workspaces(&[info("2", false), info("3", true)]);
    client.settle(&mut backend);

    let listed = client.workspaces();
    assert_eq!(listed.len(), 3);
    assert!(listed[0].removed, "workspace 1 went");
    assert!(!listed[1].removed);
    assert_eq!(listed[2].id, "3");
    assert_eq!(listed[2].state, ACTIVE);
    assert_eq!(client.workspace_group().members, 2);
}

#[test]
fn a_client_that_binds_late_is_told_of_the_workspaces_already_there() {
    let (mut backend, _first) = setup();
    backend.set_workspaces(&[info("1", false), info("2", true)]);

    let (mut late, _) = TestClient::connect(&mut backend);
    late.settle(&mut backend);

    assert_eq!(late.workspaces().len(), 2);
    assert_eq!(late.workspaces()[1].state, ACTIVE);
    assert!(late.workspace_dones() >= 1);
}

#[test]
fn a_request_to_activate_waits_for_the_commit_and_then_asks_the_core_to_view_it() {
    let (mut backend, mut client) = setup();
    backend.set_workspaces(&[info("1", true), info("2", false)]);
    client.settle(&mut backend);

    client.activate_workspace(1);
    let before_commit = client.settle(&mut backend);
    assert!(
        view_requests(&before_commit).is_empty(),
        "nothing before the commit"
    );

    client.commit_workspaces();
    let events = client.settle(&mut backend);
    assert_eq!(view_requests(&events), ["2"]);

    // The request is spent: a second commit asks for nothing.
    client.commit_workspaces();
    assert!(view_requests(&client.settle(&mut backend)).is_empty());
}

#[test]
fn a_client_that_stops_hears_no_more() {
    let (mut backend, mut client) = setup();
    backend.set_workspaces(&[info("1", true), info("2", false)]);
    client.settle(&mut backend);

    client.stop_workspace_list();
    client.settle(&mut backend);
    assert!(client.workspace_list_finished());

    backend.set_workspaces(&[info("1", false), info("2", true)]);
    client.settle(&mut backend);
    assert_eq!(client.workspaces()[1].state, 0, "it was not told");
}
