//! The monitors of the X server, as lutece outputs.

use crate::error::Result;
use lutece::{Output, OutputId, Rect};
use x11rb::connection::RequestConnection;
use x11rb::protocol::randr::{self, ConnectionExt as _, MonitorInfo, NotifyMask};
use x11rb::protocol::xproto::{ConnectionExt as _, Window};
use x11rb::rust_connection::RustConnection;

/// The RandR version that can list monitors, which is what an output is here: unlike a
/// RandR output it is the area a user sees as one screen, and a server can define its own.
const MONITORS: (u32, u32) = (1, 5);

/// Subscribes to monitor changes. Returns whether the server can list monitors; if not
/// (no RandR, or an older one), [`query`] reports the whole root window as one output.
pub(crate) fn watch(conn: &RustConnection, root: Window) -> Result<bool> {
    if conn
        .extension_information(randr::X11_EXTENSION_NAME)?
        .is_none()
    {
        return Ok(false);
    }
    let version = conn.randr_query_version(MONITORS.0, MONITORS.1)?.reply()?;
    if (version.major_version, version.minor_version) < MONITORS {
        return Ok(false);
    }
    let changes = NotifyMask::SCREEN_CHANGE | NotifyMask::OUTPUT_CHANGE | NotifyMask::CRTC_CHANGE;
    conn.randr_select_input(root, changes)?;
    Ok(true)
}

/// The outputs now: the active monitors, or the whole root window as output 0 if the
/// server has none to list.
pub(crate) fn query(conn: &RustConnection, root: Window, monitors: bool) -> Result<Vec<Output>> {
    if monitors {
        let reply = conn.randr_get_monitors(root, true)?.reply()?;
        let found: Vec<Output> = reply.monitors.iter().filter_map(output_of).collect();
        if !found.is_empty() {
            return Ok(found);
        }
    }
    let geometry = conn.get_geometry(root)?.reply()?;
    let rect = Rect::new(0, 0, u32::from(geometry.width), u32::from(geometry.height))?;
    Ok(vec![Output {
        id: OutputId::new(0),
        rect,
    }])
}

/// A monitor without an area cannot hold a window, so it is not an output. The monitor's
/// name atom is unique on the server, so it identifies the output across changes.
fn output_of(monitor: &MonitorInfo) -> Option<Output> {
    let rect = Rect::new(
        i32::from(monitor.x),
        i32::from(monitor.y),
        u32::from(monitor.width),
        u32::from(monitor.height),
    )
    .ok()?;
    Some(Output {
        id: OutputId::new(monitor.name),
        rect,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn monitor(x: i16, y: i16, width: u16, height: u16) -> MonitorInfo {
        MonitorInfo {
            name: 77,
            primary: false,
            automatic: false,
            x,
            y,
            width,
            height,
            width_in_millimeters: 0,
            height_in_millimeters: 0,
            outputs: Vec::new(),
        }
    }

    proptest! {
        #[test]
        fn a_monitor_is_an_output_exactly_when_it_has_an_area(
            x in any::<i16>(),
            y in any::<i16>(),
            width in any::<u16>(),
            height in any::<u16>(),
        ) {
            let output = output_of(&monitor(x, y, width, height));
            prop_assert_eq!(output.is_some(), width > 0 && height > 0);
            if let Some(output) = output {
                prop_assert_eq!(output.id, OutputId::new(77));
                prop_assert_eq!(output.rect.x(), i32::from(x));
                prop_assert_eq!(output.rect.height(), u32::from(height));
            }
        }
    }
}
