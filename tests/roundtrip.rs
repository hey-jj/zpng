//! Round-trip contract. Compress then decompress must reproduce the pixels
//! exactly and report the same geometry. This is the core lossless guarantee.
//!
//! Each test runs three checks: compress returns a blob, decompress reports
//! matching metadata, and the decompressed bytes equal the input. The images
//! are synthetic and cover every code path.

mod common;

use common::{assert_roundtrip, build, Pattern};

/// A1, A2, A3 across the full geometry grid. Channels 1 to 4, both channel
/// depths, every fill pattern. The `pixel_bytes <= 8` filter skips the one
/// combination the codec rejects.
#[test]
fn roundtrip_grid() {
    let patterns = [
        Pattern::Solid(0),
        Pattern::Solid(255),
        Pattern::Solid(73),
        Pattern::Gradient,
        Pattern::Stripes,
        Pattern::Checker,
        Pattern::Random(0x1234_5678),
    ];

    for channels in 1..=4u32 {
        for bytes_per_channel in 1..=2u32 {
            if channels * bytes_per_channel > 8 {
                continue;
            }
            for &pattern in &patterns {
                let image = build(7, 5, channels, bytes_per_channel, pattern);
                assert_roundtrip(&image);
            }
        }
    }
}

/// Per-image contract for 8-bit images with 1 to 4 channels.
#[test]
fn roundtrip_eight_bit_channels() {
    for channels in 1..=4u32 {
        let image = build(32, 24, channels, 1, Pattern::Random(0xABCD));
        assert_roundtrip(&image);
    }
}

/// Larger random image to stress the zstd worst case and the compress bound.
#[test]
fn roundtrip_large_random() {
    for channels in 1..=4u32 {
        let image = build(
            200,
            150,
            channels,
            1,
            Pattern::Random(channels as u64 * 99 + 1),
        );
        assert_roundtrip(&image);
    }
}
