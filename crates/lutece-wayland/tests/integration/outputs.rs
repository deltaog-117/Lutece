//! Several outputs: what the core is told and what clients can bind.

use crate::support::{TestClient, backend};
use lutece::{Event, Output, OutputId, Point, Rect, Size};
use lutece_wayland::WaylandBackend;

fn output(id: u32, x: i32, width: u32) -> Output {
    Output {
        id: OutputId::new(id),
        rect: Rect::new(x, 0, width, 600).expect("a non-empty rectangle"),
    }
}

fn reported(events: &[Event]) -> Vec<Vec<Output>> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::OutputsChanged(outputs) => Some(outputs.clone()),
            _ => None,
        })
        .collect()
}

fn two_outputs() -> (WaylandBackend, TestClient) {
    let mut backend = backend();
    backend.set_outputs(&[output(0, 0, 500), output(1, 500, 500)]);
    let (client, _) = TestClient::connect(&mut backend);
    (backend, client)
}

#[test]
fn the_core_hears_of_the_complete_set_once() {
    let mut backend = backend();
    backend.set_outputs(&[output(0, 0, 500), output(1, 500, 500)]);
    let (_client, events) = TestClient::connect(&mut backend);

    // The one-output set reported by `bind` was still waiting and is replaced.
    assert_eq!(
        reported(&events),
        vec![vec![output(0, 0, 500), output(1, 500, 500)]]
    );
}

#[test]
fn clients_can_bind_every_output() {
    let (_backend, client) = two_outputs();
    assert_eq!(client.output_count(), 2);
}

#[test]
fn an_unplugged_output_disappears_for_clients() {
    let (mut backend, mut client) = two_outputs();

    backend.set_outputs(&[output(0, 0, 1000)]);
    let events = client.settle(&mut backend);

    assert_eq!(client.output_count(), 1);
    assert_eq!(reported(&events), vec![vec![output(0, 0, 1000)]]);
}

#[test]
fn an_output_that_stays_keeps_its_global_when_it_moves() {
    let (mut backend, mut client) = two_outputs();

    backend.set_outputs(&[output(0, 0, 400), output(1, 400, 600)]);
    client.settle(&mut backend);

    assert_eq!(client.output_count(), 2);
}

#[test]
fn the_single_output_helper_leaves_one_output_of_that_size() {
    let (mut backend, mut client) = two_outputs();

    backend
        .set_output_size(Size::new(800, 600).expect("non-zero"))
        .expect("the size fits");
    let events = client.settle(&mut backend);

    assert_eq!(client.output_count(), 1);
    assert_eq!(
        reported(&events),
        vec![vec![Output {
            id: OutputId::new(0),
            rect: Rect::from_parts(Point { x: 0, y: 0 }, Size::new(800, 600).expect("non-zero")),
        }]]
    );
}
