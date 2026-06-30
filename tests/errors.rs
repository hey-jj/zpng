//! Failure paths. Decompress reports a typed error for malformed input.
//! Compress reports one for geometry it cannot encode.

mod common;

use common::{build, Pattern};
use zpng::{CompressError, DecodeError};

/// Empty input is shorter than the 8-byte header.
#[test]
fn decompress_empty() {
    assert_eq!(zpng::decompress(&[]), Err(DecodeError::TooShort));
}

/// Seven bytes is one short of a header.
#[test]
fn decompress_too_short() {
    assert_eq!(zpng::decompress(&[0u8; 7]), Err(DecodeError::TooShort));
}

/// Exactly 8 bytes but the wrong magic.
#[test]
fn decompress_bad_magic() {
    assert_eq!(zpng::decompress(&[0u8; 8]), Err(DecodeError::BadMagic));
    // Right length, magic bytes swapped.
    let blob = [0xFB, 0xF8, 0, 0, 0, 0, 1, 1];
    assert_eq!(zpng::decompress(&blob), Err(DecodeError::BadMagic));
}

/// Valid header, garbage where the zstd frame should be.
#[test]
fn decompress_garbage_frame() {
    let mut blob = vec![0xF8, 0xFB, 4, 0, 4, 0, 3, 1];
    blob.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11, 0x22]);
    assert!(zpng::decompress(&blob).is_err());
}

/// Valid header but the frame decodes to fewer bytes than the geometry needs.
#[test]
fn decompress_size_mismatch() {
    // Header claims a 4x4 RGB image, 48 bytes, but the frame holds 6 bytes.
    let frame = zstd::bulk::compress(&[1u8, 2, 3, 4, 5, 6], 1).expect("zstd");
    let mut blob = vec![0xF8, 0xFB, 4, 0, 4, 0, 3, 1];
    blob.extend_from_slice(&frame);
    assert_eq!(
        zpng::decompress(&blob),
        Err(DecodeError::SizeMismatch {
            expected: 48,
            got: 6,
        })
    );
}

/// A truncated valid blob fails to decode.
#[test]
fn decompress_truncated() {
    let image = build(16, 16, 3, 1, Pattern::Random(7));
    let blob = zpng::compress(&image).expect("compress");
    let cut = &blob[..blob.len() - 4];
    assert!(zpng::decompress(cut).is_err());
}

/// Pixel byte width above 8 is rejected at compress time.
#[test]
fn compress_rejects_wide_pixels() {
    // channels=4, bytes_per_channel=4 -> pixel_bytes=16.
    let mut image = build(2, 2, 4, 2, Pattern::Solid(0));
    image.bytes_per_channel = 4;
    image.buffer = vec![0u8; 2 * 2 * 4 * 4];
    assert_eq!(
        zpng::compress(&image),
        Err(CompressError::PixelTooWide { pixel_bytes: 16 })
    );
}

/// Decompress also rejects a forged header that claims a pixel byte width above
/// 8. Compress can never produce one, so the only source is a crafted blob. The
/// codec rejects it rather than skipping the unfilter and returning a zeroed
/// buffer.
#[test]
fn decompress_rejects_wide_pixels() {
    // Forge channels=4, bytes_per_channel=4 -> pixel_bytes=16, 1x1 -> 16 bytes.
    let frame = zstd::bulk::compress(&[7u8; 16], 1).expect("zstd");
    let mut blob = vec![0xF8, 0xFB, 1, 0, 1, 0, 4, 4];
    blob.extend_from_slice(&frame);
    assert_eq!(zpng::decompress(&blob), Err(DecodeError::BadGeometry));
}

/// A forged header whose pixel count overflows 32-bit math is rejected before
/// any allocation or unfilter. This is the case that once panicked with an
/// out-of-bounds index: width*height*pixel_bytes wrapped to a small value while
/// the plane offset math stayed large.
#[test]
fn decompress_rejects_overflowing_geometry() {
    // width*height*3 = 4_294_967_346, just past u32::MAX.
    let frame = zstd::bulk::compress(&[0u8; 50], 1).expect("zstd");
    let mut blob = vec![0xF8u8, 0xFB];
    blob.extend_from_slice(&23307u16.to_le_bytes());
    blob.extend_from_slice(&61426u16.to_le_bytes());
    blob.push(3);
    blob.push(1);
    blob.extend_from_slice(&frame);
    assert_eq!(zpng::decompress(&blob), Err(DecodeError::BadGeometry));
}

/// Compress rejects a buffer that does not match the declared geometry instead
/// of indexing past it.
#[test]
fn compress_rejects_undersized_buffer() {
    let mut image = build(4, 4, 3, 1, Pattern::Solid(0));
    image.buffer = vec![0u8; 10]; // geometry needs 48
    assert_eq!(
        zpng::compress(&image),
        Err(CompressError::BufferSizeMismatch {
            expected: 48,
            got: 10,
        })
    );
}

/// Compress rejects geometry that overflows 32-bit pixel math rather than
/// looping for billions of iterations. The dimensions fit the u16 header fields,
/// but `width * height * pixel_bytes` (65535 * 65535 * 2) exceeds u32.
#[test]
fn compress_rejects_overflowing_geometry() {
    let mut image = build(1, 1, 1, 2, Pattern::Solid(0));
    image.width_pixels = 65535;
    image.height_pixels = 65535;
    assert_eq!(zpng::compress(&image), Err(CompressError::GeometryOverflow));
}
