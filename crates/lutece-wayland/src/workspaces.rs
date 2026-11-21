//! The workspace list for bars: `ext_workspace_manager_v1`.
//!
//! A bar binds the manager and is told about every workspace: its name, and whether it is the
//! active one. It is told again when that changes, and when a workspace appears or goes. A request
//! to activate a workspace waits for the manager's `commit`, as the protocol says, and then
//! reaches the core as a request to view that workspace.
//!
//! The core has one pool of workspaces that every output draws from, so a client is given one
//! group holding every workspace and every output, and a workspace is active when some output
//! shows it. The group cannot create workspaces, and a workspace cannot be deactivated, removed
//! or assigned: only `activate` is offered. Outputs are those at the time the client binds.
//!
//! What a client is to be told is worked out from plain data ([`plan`]), so that part is tested
//! on its own.

use crate::compositor::State;
use lutece::{Action, Event, Tag};
use smithay::reexports::wayland_server::backend::ClientId;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};
use wayland_protocols::ext::workspace::v1::server::ext_workspace_group_handle_v1::{
    self, ExtWorkspaceGroupHandleV1, GroupCapabilities,
};
use wayland_protocols::ext::workspace::v1::server::ext_workspace_handle_v1::{
    self, ExtWorkspaceHandleV1, WorkspaceCapabilities,
};
use wayland_protocols::ext::workspace::v1::server::ext_workspace_manager_v1::{
    self, ExtWorkspaceManagerV1,
};

/// `ext_workspace_handle_v1.state.active`.
const STATE_ACTIVE: u32 = 1;
/// `ext_workspace_handle_v1.workspace_capabilities.activate`.
const CAPABILITY_ACTIVATE: u32 = 1;

/// One workspace as the core has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInfo {
    /// The workspace's tag, which is its name and its id on the protocol.
    pub tag: String,
    /// Whether an output shows it.
    pub active: bool,
}

/// The `state` value for a workspace.
const fn state_bits(active: bool) -> u32 {
    if active { STATE_ACTIVE } else { 0 }
}

/// The `state` event's value for a workspace.
fn state_of(active: bool) -> ext_workspace_handle_v1::State {
    ext_workspace_handle_v1::State::from_bits_truncate(state_bits(active))
}

/// What a client must be told to go from what it knows to `next`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Plan {
    /// Tags the client does not know, in the order of `next`.
    pub(crate) added: Vec<String>,
    /// Tags the client knows that are gone.
    pub(crate) removed: Vec<String>,
    /// Tags the client knows whose active state differs, with the new state.
    pub(crate) changed: Vec<(String, bool)>,
}

impl Plan {
    /// Whether the client already knows everything.
    pub(crate) fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

/// What differs between `known` (a client's tags with their active state) and `next`.
pub(crate) fn plan(known: &[(String, bool)], next: &[WorkspaceInfo]) -> Plan {
    let mut plan = Plan::default();
    for (tag, _) in known {
        if !next.iter().any(|info| &info.tag == tag) {
            plan.removed.push(tag.clone());
        }
    }
    for info in next {
        match known.iter().find(|(tag, _)| *tag == info.tag) {
            None => plan.added.push(info.tag.clone()),
            Some((_, active)) if *active != info.active => {
                plan.changed.push((info.tag.clone(), info.active));
            }
            Some(_) => {}
        }
    }
    plan
}

/// A workspace a client holds a handle for, and what it was last told of its state.
#[derive(Debug)]
struct Known {
    tag: String,
    active: bool,
    handle: ExtWorkspaceHandleV1,
}

/// A client that bound the manager.
#[derive(Debug)]
struct Subscriber {
    manager: ExtWorkspaceManagerV1,
    group: ExtWorkspaceGroupHandleV1,
    handles: Vec<Known>,
    /// Workspaces asked for since the last `commit`.
    pending: Vec<String>,
}

impl Subscriber {
    /// Tells the client what it lacks to match `list`, and that the batch is complete.
    fn sync(&mut self, display: &DisplayHandle, list: &[WorkspaceInfo]) {
        let known: Vec<(String, bool)> = self
            .handles
            .iter()
            .map(|known| (known.tag.clone(), known.active))
            .collect();
        let plan = plan(&known, list);
        if plan.is_empty() {
            return;
        }
        for tag in &plan.removed {
            if let Some(at) = self.handles.iter().position(|known| &known.tag == tag) {
                let gone = self.handles.remove(at);
                self.group.workspace_leave(&gone.handle);
                gone.handle.removed();
            }
        }
        for tag in &plan.added {
            let active = list.iter().any(|info| &info.tag == tag && info.active);
            if let Some(handle) = self.introduce(display, tag, active) {
                self.handles.push(Known {
                    tag: tag.clone(),
                    active,
                    handle,
                });
            }
        }
        for (tag, active) in &plan.changed {
            if let Some(known) = self.handles.iter_mut().find(|known| &known.tag == tag) {
                known.active = *active;
                known.handle.state(state_of(*active));
            }
        }
        self.manager.done();
    }

    /// Makes the handle for the workspace `tag` and tells the client everything about it.
    fn introduce(
        &self,
        display: &DisplayHandle,
        tag: &str,
        active: bool,
    ) -> Option<ExtWorkspaceHandleV1> {
        let client = self.manager.client()?;
        let handle = client
            .create_resource::<ExtWorkspaceHandleV1, String, State>(
                display,
                self.manager.version(),
                tag.to_owned(),
            )
            .ok()?;
        self.manager.workspace(&handle);
        handle.id(tag.to_owned());
        handle.name(tag.to_owned());
        handle.coordinates(Vec::new());
        handle.capabilities(WorkspaceCapabilities::from_bits_truncate(
            CAPABILITY_ACTIVATE,
        ));
        handle.state(state_of(active));
        self.group.workspace_enter(&handle);
        Some(handle)
    }
}

/// The manager global, the clients bound to it, and the workspaces it lists.
#[derive(Debug, Default)]
pub(crate) struct ExtWorkspaces {
    subscribers: Vec<Subscriber>,
    list: Vec<WorkspaceInfo>,
}

impl ExtWorkspaces {
    /// Creates the global on `display`.
    pub(crate) fn new(display: &DisplayHandle) -> Self {
        // Only the global matters: it stays up for as long as the display does.
        let _ = display.create_global::<State, ExtWorkspaceManagerV1, ()>(1, ());
        Self::default()
    }

    /// Replaces the workspaces the core has, and tells every client what changed.
    pub(crate) fn set(&mut self, display: &DisplayHandle, list: Vec<WorkspaceInfo>) {
        self.list = list;
        self.subscribers
            .retain(|subscriber| subscriber.manager.is_alive());
        for subscriber in &mut self.subscribers {
            subscriber.sync(display, &self.list);
        }
    }

    /// Puts the group of the client that owns `wl_output` on that output.
    pub(crate) fn enter_output(&mut self, wl_output: &WlOutput) {
        for subscriber in &mut self.subscribers {
            if wl_output.id().same_client_as(&subscriber.manager.id()) {
                subscriber.group.output_enter(wl_output);
                subscriber.manager.done();
            }
        }
    }

    fn subscriber_of(&mut self, manager: &ExtWorkspaceManagerV1) -> Option<&mut Subscriber> {
        self.subscribers
            .iter_mut()
            .find(|subscriber| &subscriber.manager == manager)
    }
}

impl GlobalDispatch<ExtWorkspaceManagerV1, (), State> for State {
    fn bind(
        state: &mut State,
        display: &DisplayHandle,
        client: &Client,
        resource: New<ExtWorkspaceManagerV1>,
        (): &(),
        data_init: &mut DataInit<'_, State>,
    ) {
        let manager = data_init.init(resource, ());
        let Ok(group) = client.create_resource::<ExtWorkspaceGroupHandleV1, (), State>(
            display,
            manager.version(),
            (),
        ) else {
            return;
        };
        manager.workspace_group(&group);
        group.capabilities(GroupCapabilities::empty());
        for (_, output) in &state.wl_outputs {
            for resource in output.client_outputs(client) {
                group.output_enter(&resource);
            }
        }
        let mut subscriber = Subscriber {
            manager,
            group,
            handles: Vec::new(),
            pending: Vec::new(),
        };
        subscriber.sync(display, &state.workspaces.list);
        // A client that binds before there is anything to list still gets its `done`.
        if state.workspaces.list.is_empty() {
            subscriber.manager.done();
        }
        state.workspaces.subscribers.push(subscriber);
    }
}

impl Dispatch<ExtWorkspaceManagerV1, (), State> for State {
    fn request(
        state: &mut State,
        _client: &Client,
        manager: &ExtWorkspaceManagerV1,
        request: ext_workspace_manager_v1::Request,
        (): &(),
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        match request {
            // The requests since the last commit take effect together: each asks the core to
            // view a workspace.
            ext_workspace_manager_v1::Request::Commit => {
                let asked = state
                    .workspaces
                    .subscriber_of(manager)
                    .map(|subscriber| std::mem::take(&mut subscriber.pending))
                    .unwrap_or_default();
                for tag in asked {
                    if let Ok(tag) = Tag::try_new(&tag) {
                        state
                            .events
                            .push(Event::ActionRequested(Action::ViewTag(tag)));
                    }
                }
            }
            ext_workspace_manager_v1::Request::Stop => {
                state
                    .workspaces
                    .subscribers
                    .retain(|subscriber| &subscriber.manager != manager);
                manager.finished();
            }
            _ => {}
        }
    }

    fn destroyed(state: &mut State, _client: ClientId, manager: &ExtWorkspaceManagerV1, (): &()) {
        state
            .workspaces
            .subscribers
            .retain(|subscriber| &subscriber.manager != manager);
    }
}

impl Dispatch<ExtWorkspaceGroupHandleV1, (), State> for State {
    fn request(
        _state: &mut State,
        _client: &Client,
        _group: &ExtWorkspaceGroupHandleV1,
        request: ext_workspace_group_handle_v1::Request,
        (): &(),
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        // `create_workspace` is not offered (the group says no capabilities), and `destroy` needs
        // nothing: the client's record goes with its manager.
        let _ = request;
    }
}

impl Dispatch<ExtWorkspaceHandleV1, String, State> for State {
    fn request(
        state: &mut State,
        _client: &Client,
        handle: &ExtWorkspaceHandleV1,
        request: ext_workspace_handle_v1::Request,
        tag: &String,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, State>,
    ) {
        // Only `activate` is offered. It waits for the manager's `commit`.
        if matches!(request, ext_workspace_handle_v1::Request::Activate) {
            for subscriber in &mut state.workspaces.subscribers {
                if subscriber
                    .handles
                    .iter()
                    .any(|known| &known.handle == handle)
                {
                    subscriber.pending.push(tag.clone());
                }
            }
        }
    }

    fn destroyed(
        state: &mut State,
        _client: ClientId,
        handle: &ExtWorkspaceHandleV1,
        _tag: &String,
    ) {
        for subscriber in &mut state.workspaces.subscribers {
            subscriber.handles.retain(|known| &known.handle != handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn info(tag: &str, active: bool) -> WorkspaceInfo {
        WorkspaceInfo {
            tag: tag.to_owned(),
            active,
        }
    }

    #[test]
    fn a_client_that_knows_nothing_is_told_every_workspace_in_order() {
        let plan = plan(&[], &[info("1", true), info("2", false)]);
        assert_eq!(plan.added, ["1", "2"]);
        assert!(plan.removed.is_empty() && plan.changed.is_empty());
    }

    #[test]
    fn a_client_that_knows_everything_is_told_nothing() {
        let known = [("1".to_owned(), true), ("2".to_owned(), false)];
        assert!(plan(&known, &[info("1", true), info("2", false)]).is_empty());
    }

    #[test]
    fn a_change_of_which_is_active_is_two_changes() {
        let known = [("1".to_owned(), true), ("2".to_owned(), false)];
        let plan = plan(&known, &[info("1", false), info("2", true)]);
        assert_eq!(
            plan.changed,
            [("1".to_owned(), false), ("2".to_owned(), true)]
        );
    }

    #[test]
    fn a_workspace_that_goes_and_one_that_comes_are_told() {
        let known = [("1".to_owned(), false), ("2".to_owned(), false)];
        let plan = plan(&known, &[info("2", false), info("3", false)]);
        assert_eq!(plan.removed, ["1"]);
        assert_eq!(plan.added, ["3"]);
    }

    #[test]
    fn the_state_is_active_or_nothing() {
        assert_eq!(state_bits(true), 1);
        assert_eq!(state_bits(false), 0);
    }

    proptest! {
        /// Applying a plan to what a client knows gives exactly the new list.
        #[test]
        fn applying_the_plan_gives_the_new_list(
            before in proptest::collection::btree_map("[a-c]{1,2}", any::<bool>(), 0..6),
            after in proptest::collection::btree_map("[a-c]{1,2}", any::<bool>(), 0..6),
        ) {
            let known: Vec<(String, bool)> = before.clone().into_iter().collect();
            let next: Vec<WorkspaceInfo> = after
                .iter()
                .map(|(tag, active)| info(tag, *active))
                .collect();
            let plan = plan(&known, &next);

            let mut state = before;
            for tag in &plan.removed {
                prop_assert!(state.remove(tag).is_some());
            }
            for tag in &plan.added {
                prop_assert!(state.insert(tag.clone(), after[tag]).is_none());
            }
            for (tag, active) in &plan.changed {
                state.insert(tag.clone(), *active);
            }
            prop_assert_eq!(state, after);
        }

        #[test]
        fn what_a_client_already_has_is_never_told_again(
            list in proptest::collection::btree_map("[a-c]{1,2}", any::<bool>(), 0..6),
        ) {
            let known: Vec<(String, bool)> = list.clone().into_iter().collect();
            let next: Vec<WorkspaceInfo> = list.iter().map(|(t, a)| info(t, *a)).collect();
            prop_assert!(plan(&known, &next).is_empty());
        }
    }
}
