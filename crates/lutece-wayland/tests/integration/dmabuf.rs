//! GPU buffers: the global exists only once the host described its GPU, names that GPU,
//! and takes the buffers in the formats it offered.

use crate::support::{TestClient, backend};
use lutece_wayland::DmabufSupport;

const XRGB8888: u32 = 0x3432_5258; // fourcc "XR24"
const ARGB8888: u32 = 0x3432_5241; // fourcc "AR24"
const LINEAR: u64 = 0;
/// A device number as the protocol carries it: the bytes of a `dev_t`.
const DEVICE: u64 = 0xE280;

fn support() -> DmabufSupport {
    DmabufSupport::new(DEVICE, [(XRGB8888, LINEAR)])
}

#[test]
fn without_a_gpu_described_there_is_no_dmabuf() {
    let mut backend = backend();
    let (client, _) = TestClient::connect(&mut backend);

    assert!(!client.has_dmabuf());
}

#[test]
fn once_described_clients_can_bind_it() {
    let mut backend = backend();
    backend.enable_dmabuf(&support()).expect("feedback builds");
    let (client, _) = TestClient::connect(&mut backend);

    assert!(client.has_dmabuf());
}

#[test]
fn the_feedback_names_the_gpu_and_lists_the_formats() {
    let mut backend = backend();
    backend.enable_dmabuf(&support()).expect("feedback builds");
    let (mut client, _) = TestClient::connect(&mut backend);

    client.request_dmabuf_feedback();
    client.settle(&mut backend);

    assert_eq!(
        client.dmabuf_device().map(<[u8]>::to_vec),
        Some(DEVICE.to_ne_bytes().to_vec())
    );
    assert_eq!(client.dmabuf_table_entries(), Some(1));
}

#[test]
fn a_buffer_in_an_offered_format_is_accepted() {
    let mut backend = backend();
    backend.enable_dmabuf(&support()).expect("feedback builds");
    let (mut client, _) = TestClient::connect(&mut backend);

    client.offer_dmabuf(64, 32, XRGB8888, LINEAR);
    client.settle(&mut backend);

    assert_eq!(client.dmabuf_results(), [true]);
}

#[test]
fn describing_the_gpu_again_replaces_the_offer() {
    let mut backend = backend();
    backend.enable_dmabuf(&support()).expect("feedback builds");
    backend
        .enable_dmabuf(&DmabufSupport::new(DEVICE, [(ARGB8888, LINEAR)]))
        .expect("feedback builds");
    let (mut client, _) = TestClient::connect(&mut backend);

    // The format only the second description offers. The first one's would end the
    // connection with a protocol error, which is Smithay's doing and not tested here.
    client.offer_dmabuf(64, 32, ARGB8888, LINEAR);
    client.settle(&mut backend);

    assert_eq!(client.dmabuf_results(), [true]);
}
