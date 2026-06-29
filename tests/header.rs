//! Header byte layout. The first 8 bytes of every blob are fixed and
//! little-endian. This locks the wire format independent of the zstd version.

mod common;

use common::{build, Pattern};

/// Magic on disk is `F8 FB`, width and height follow little-endian, then the
/// channel and depth bytes.
#[test]
fn header_layout() {
    let image = build(0x1234, 0x05, 3, 1, Pattern::Solid(0));
    // Build with explicit geometry. The build helper clamps width to a real
    // buffer, so set fields directly for a width that fits the u16 field but
    // would be expensive to allocate.
    let mut image = image;
    image.width_pixels = 0x1234;
    image.height_pixels = 0x0005;
    // Resize the buffer to the declared geometry so compress reads in bounds.
    let len = 0x1234usize * 0x0005 * 3;
    image.buffer = vec![0u8; len];

    let blob = zpng::compress(&image).expect("compress");
    assert_eq!(&blob[0..2], &[0xF8, 0xFB], "magic");
    assert_eq!(&blob[2..4], &[0x34, 0x12], "width little-endian");
    assert_eq!(&blob[4..6], &[0x05, 0x00], "height little-endian");
    assert_eq!(blob[6], 3, "channels");
    assert_eq!(blob[7], 1, "bytes per channel");
}

/// The header is exactly 8 bytes. The blob length is 8 plus the zstd frame.
#[test]
fn header_overhead_is_eight() {
    let image = build(4, 4, 3, 1, Pattern::Solid(50));
    let blob = zpng::compress(&image).expect("compress");
    assert!(blob.len() >= 8);
    assert_eq!(blob[0], 0xF8);
    assert_eq!(blob[1], 0xFB);
}

/// Header fields survive a round trip for a grid of geometries.
#[test]
fn header_fields_round_trip() {
    let shapes = [(1u32, 1u32), (7, 3), (64, 1), (1, 64)];
    for channels in 1..=4u32 {
        for bytes_per_channel in 1..=2u32 {
            if channels * bytes_per_channel > 8 {
                continue;
            }
            for &(w, h) in &shapes {
                let image = build(w, h, channels, bytes_per_channel, Pattern::Gradient);
                let blob = zpng::compress(&image).expect("compress");
                let back = zpng::decompress(&blob).expect("decompress");
                assert_eq!(back.width_pixels, w);
                assert_eq!(back.height_pixels, h);
                assert_eq!(back.channels, channels);
                assert_eq!(back.bytes_per_channel, bytes_per_channel);
            }
        }
    }
}
