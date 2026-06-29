//! Filter behavior through the public API.
//!
//! The pack and unpack functions are private. These tests pin the observable
//! effects: a blob built from known planes decodes to known pixels, and the
//! filter is a true inverse for random rows. The hand-computed byte vectors
//! live in the unit tests inside the filter module.

mod common;

use common::{build, Pattern};

/// Build a blob by hand from a header and a packed plane buffer, then confirm
/// decompress applies the inverse RGB transform and plane split correctly.
///
/// Planes Y=[30,3], U=[246,5], V=[10,3] are the forward result for input
/// pixels `(10,20,30)` and `(15,28,33)` on a 2x1 RGB image. Decompress must
/// return those pixels.
#[test]
fn decode_known_rgb_planes() {
    let packed = [30u8, 3, 246, 5, 10, 3];
    let frame = zstd::bulk::compress(&packed, 1).expect("zstd compress");

    let mut blob = Vec::new();
    // Header: magic F8 FB, width=2, height=1, channels=3, bpc=1.
    blob.extend_from_slice(&[0xF8, 0xFB, 2, 0, 1, 0, 3, 1]);
    blob.extend_from_slice(&frame);

    let back = zpng::decompress(&blob).expect("decompress");
    assert_eq!(back.buffer, vec![10, 20, 30, 15, 28, 33]);
    assert_eq!(back.width_pixels, 2);
    assert_eq!(back.height_pixels, 1);
    assert_eq!(back.channels, 3);
}

/// The filter is an exact inverse for every channel and depth combination on
/// random data. This is the identity property at the codec layer.
#[test]
fn filter_is_identity() {
    for channels in 1..=4u32 {
        for bytes_per_channel in 1..=2u32 {
            if channels * bytes_per_channel > 8 {
                continue;
            }
            let image = build(9, 6, channels, bytes_per_channel, Pattern::Random(0xFEED));
            let blob = zpng::compress(&image).expect("compress");
            let back = zpng::decompress(&blob).expect("decompress");
            assert_eq!(back.buffer, image.buffer);
        }
    }
}

/// Generic path widths 5 through 8 are reached via 16-bit channels. Confirm
/// each round-trips.
#[test]
fn generic_wide_pixels() {
    // pixel_bytes 6 (3 channels x 2 bytes) and 8 (4 channels x 2 bytes).
    for channels in [3u32, 4] {
        let image = build(8, 8, channels, 2, Pattern::Gradient);
        let blob = zpng::compress(&image).expect("compress");
        let back = zpng::decompress(&blob).expect("decompress");
        assert_eq!(back.buffer, image.buffer);
    }
}
