//! Which outputs a client's surfaces are told they are on, and the integer scale that
//! goes with it, for clients that read `wl_output.scale` instead of `wp_fractional_scale`.

use crate::support::{TestClient, backend};
use lutece::{Output, OutputId, Placement, Rect, Scene, WinId};
use lutece_wayland::{Scale, WaylandBackend};

fn output(id: u32, x: i32) -> Output {
    Output {
        id: OutputId::new(id),
        rect: Rect::new(x, 0, 500, 600).expect("a non-empty rectangle"),
    }
}

fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
    Rect::new(x, y, width, height).expect("a non-empty rectangle")
}

/// Two outputs side by side, `[0, 500)` and `[500, 1000)`, and one mapped window.
fn one_window() -> (WaylandBackend, TestClient, usize, WinId) {
    let mut backend = backend();
    backend.set_outputs(&[output(0, 0), output(1, 500)]);
    let (mut client, mut events) = TestClient::connect(&mut backend);
    let window = client.open_window("window", "app");
    events.extend(client.settle(&mut backend));
    client.map_window(window, 100, 100);
    events.extend(client.settle(&mut backend));
    let id = events
        .iter()
        .find_map(|event| match event {
            lutece::Event::WindowOpened { id, .. } => Some(*id),
            _ => None,
        })
        .expect("the window was announced");
    (backend, client, window, id)
}

fn place(backend: &mut WaylandBackend, client: &mut TestClient, id: WinId, at: Option<Rect>) {
    let placements = at.map(|rect| Placement { id, rect }).into_iter().collect();
    let scene = Scene::try_new(placements, None).expect("a valid scene");
    backend.apply_scene(&scene).expect("scene applies");
    client.settle(backend);
}

#[test]
fn a_window_is_on_the_output_it_sits_in() {
    let (mut backend, mut client, window, id) = one_window();

    place(&mut backend, &mut client, id, Some(rect(10, 10, 200, 200)));

    assert_eq!(client.outputs_of_window(window), vec![0]);
}

#[test]
fn a_window_across_the_seam_is_on_both_outputs() {
    let (mut backend, mut client, window, id) = one_window();

    place(&mut backend, &mut client, id, Some(rect(400, 10, 200, 200)));

    assert_eq!(client.outputs_of_window(window), vec![0, 500]);
}

#[test]
fn a_window_that_only_touches_an_output_is_not_on_it() {
    let (mut backend, mut client, window, id) = one_window();

    place(&mut backend, &mut client, id, Some(rect(300, 10, 200, 200)));

    assert_eq!(client.outputs_of_window(window), vec![0]);
}

#[test]
fn moving_a_window_to_the_other_output_leaves_the_first() {
    let (mut backend, mut client, window, id) = one_window();
    place(&mut backend, &mut client, id, Some(rect(10, 10, 200, 200)));

    place(&mut backend, &mut client, id, Some(rect(600, 10, 200, 200)));

    assert_eq!(client.outputs_of_window(window), vec![500]);
}

#[test]
fn a_hidden_window_is_on_no_output() {
    let (mut backend, mut client, window, id) = one_window();
    place(&mut backend, &mut client, id, Some(rect(10, 10, 200, 200)));

    place(&mut backend, &mut client, id, None);

    assert_eq!(client.outputs_of_window(window), Vec::<i32>::new());
}

#[test]
fn an_unplugged_output_is_left() {
    let (mut backend, mut client, window, id) = one_window();
    place(&mut backend, &mut client, id, Some(rect(400, 10, 200, 200)));

    backend.set_outputs(&[output(0, 0)]);
    client.settle(&mut backend);

    assert_eq!(client.outputs_of_window(window), vec![0]);
}

#[test]
fn an_output_that_grows_over_a_window_is_entered() {
    let (mut backend, mut client, window, id) = one_window();
    place(&mut backend, &mut client, id, Some(rect(600, 10, 200, 200)));
    assert_eq!(client.outputs_of_window(window), vec![500]);

    // The first output now reaches under the window as well.
    backend.set_outputs(&[
        Output {
            id: OutputId::new(0),
            rect: rect(0, 0, 700, 600),
        },
        output(1, 500),
    ]);
    client.settle(&mut backend);

    assert_eq!(client.outputs_of_window(window), vec![0, 500]);
}

#[test]
fn the_buffer_scale_is_the_host_scale_rounded_up() {
    let (mut backend, mut client, window, id) = one_window();
    backend.set_scale(Scale::from_factor(1.5).expect("a usable scale"));

    place(&mut backend, &mut client, id, Some(rect(10, 10, 200, 200)));

    assert_eq!(client.window(window).buffer_scale, Some(2));
}

#[test]
fn a_change_of_scale_reaches_a_window_already_placed() {
    let (mut backend, mut client, window, id) = one_window();
    backend.set_scale(Scale::from_factor(2.0).expect("a usable scale"));
    place(&mut backend, &mut client, id, Some(rect(10, 10, 200, 200)));
    assert_eq!(client.window(window).buffer_scale, Some(2));

    backend.set_scale(Scale::ONE);
    client.settle(&mut backend);

    assert_eq!(client.window(window).buffer_scale, Some(1));
}

#[test]
fn a_popup_is_on_the_outputs_of_its_window() {
    let (mut backend, mut client, window, id) = one_window();
    place(&mut backend, &mut client, id, Some(rect(400, 10, 200, 200)));

    let popup = client.open_popup(window, (10, 10), (50, 30), false);
    client.settle(&mut backend);
    client.map_popup(popup, 50, 30);
    client.settle(&mut backend);

    assert_eq!(client.outputs_of_popup(popup), vec![0, 500]);
}
