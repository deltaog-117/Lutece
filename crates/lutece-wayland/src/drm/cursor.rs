//! The pointer, drawn by the compositor.
//!
//! A window system shows a client's named cursor ("arrow", "text") by itself; a screen of
//! its own has none, so without this the pointer would be invisible unless a client sent its
//! own picture. This is one built-in arrow, drawn at twice its size, for every named cursor.

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::Transform;

/// The arrow: `X` is outline, `.` is fill, a space is nothing. Its tip is the top left.
const ARROW: [&str; 16] = [
    "X          ",
    "XX         ",
    "X.X        ",
    "X..X       ",
    "X...X      ",
    "X....X     ",
    "X.....X    ",
    "X......X   ",
    "X.......X  ",
    "X........X ",
    "X.....XXXXX",
    "X..X..X    ",
    "X.X X..X   ",
    "XX  X..X   ",
    "X    X..X  ",
    "     XXXX  ",
];

/// Each pixel of [`ARROW`] is this many on the screen, so the pointer is easy to see on a
/// dense display.
const GROW: usize = 2;

/// The arrow as `width`, `height` and premultiplied R, G, B, A bytes.
pub(crate) fn arrow() -> (usize, usize, Vec<u8>) {
    let columns = ARROW[0].len();
    let (width, height) = (columns * GROW, ARROW.len() * GROW);
    let mut pixels = Vec::with_capacity(width * height * 4);
    for row in ARROW {
        let line: Vec<[u8; 4]> = row
            .bytes()
            .flat_map(|cell| {
                let pixel = match cell {
                    b'X' => [0, 0, 0, 255],
                    b'.' => [255, 255, 255, 255],
                    _ => [0, 0, 0, 0],
                };
                [pixel; GROW]
            })
            .collect();
        for _ in 0..GROW {
            pixels.extend(line.iter().flatten());
        }
    }
    (width, height, pixels)
}

/// The arrow as a picture the renderer can draw, and its size in pixels.
pub(crate) fn sprite() -> (MemoryRenderBuffer, (i32, i32)) {
    let (width, height, pixels) = arrow();
    // The sizes are a few dozen, far inside `i32`.
    let size = (
        i32::try_from(width).unwrap_or(i32::MAX),
        i32::try_from(height).unwrap_or(i32::MAX),
    );
    let buffer = MemoryRenderBuffer::from_slice(
        &pixels,
        // Bytes in memory are R, G, B, A, which is what DRM calls ABGR.
        Fourcc::Abgr8888,
        size,
        1,
        Transform::Normal,
        None,
    );
    (buffer, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_of_the_arrow_is_as_wide_as_the_first() {
        assert!(ARROW.iter().all(|row| row.len() == ARROW[0].len()));
    }

    #[test]
    fn the_picture_has_a_pixel_of_four_bytes_for_every_cell_grown() {
        let (width, height, pixels) = arrow();

        assert_eq!(width, ARROW[0].len() * GROW);
        assert_eq!(height, ARROW.len() * GROW);
        assert_eq!(pixels.len(), width * height * 4);
    }

    #[test]
    fn the_tip_is_the_top_left_pixel_and_it_is_drawn() {
        let (_, _, pixels) = arrow();

        // Outline, fully opaque: the pointer's hotspot is a visible pixel.
        assert_eq!(&pixels[..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn a_premultiplied_pixel_never_has_a_channel_above_its_alpha() {
        let (_, _, pixels) = arrow();

        assert!(pixels.chunks(4).all(|p| p[..3].iter().all(|c| *c <= p[3])));
    }
}
