//! Failure paths. Decompress returns `None` for malformed input. Compress
//! returns `None` only when the pixel byte width exceeds 8.

mod common;

use common::{build, Pattern};

/// Empty input is shorter than the 8-byte header.
#[test]
fn decompress_empty() {
    assert!(zpng::decompress(&[]).is_none());
}

/// Seven bytes is one short of a header.
#[test]
fn decompress_too_short() {
    assert!(zpng::decompress(&[0u8; 7]).is_none());
}

/// Exactly 8 bytes but the wrong magic.
#[test]
fn decompress_bad_magic() {
    assert!(zpng::decompress(&[0u8; 8]).is_none());
    // Right length, magic bytes swapped.
    let blob = [0xFB, 0xF8, 0, 0, 0, 0, 1, 1];
    assert!(zpng::decompress(&blob).is_none());
}

/// Valid header, garbage where the zstd frame should be.
#[test]
fn decompress_garbage_frame() {
    let mut blob = vec![0xF8, 0xFB, 4, 0, 4, 0, 3, 1];
    blob.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11, 0x22]);
    assert!(zpng::decompress(&blob).is_none());
}

/// Valid header but the frame decodes to fewer bytes than the geometry needs.
#[test]
fn decompress_size_mismatch() {
    // Header claims a 4x4 RGB image, 48 bytes, but the frame holds 6 bytes.
    let frame = zstd::bulk::compress(&[1u8, 2, 3, 4, 5, 6], 1).expect("zstd");
    let mut blob = vec![0xF8, 0xFB, 4, 0, 4, 0, 3, 1];
    blob.extend_from_slice(&frame);
    assert!(zpng::decompress(&blob).is_none());
}

/// A truncated valid blob fails to decode.
#[test]
fn decompress_truncated() {
    let image = build(16, 16, 3, 1, Pattern::Random(7));
    let blob = zpng::compress(&image).expect("compress");
    let cut = &blob[..blob.len() - 4];
    assert!(zpng::decompress(cut).is_none());
}

/// Pixel byte width above 8 is rejected at compress time.
#[test]
fn compress_rejects_wide_pixels() {
    // channels=4, bytes_per_channel=4 -> pixel_bytes=16.
    let mut image = build(2, 2, 4, 2, Pattern::Solid(0));
    image.bytes_per_channel = 4;
    image.buffer = vec![0u8; 2 * 2 * 4 * 4];
    assert!(zpng::compress(&image).is_none());
}
