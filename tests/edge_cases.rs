//! Degenerate and boundary geometries. Zero-area images, single rows and
//! columns, the maximum accepted pixel width, and header truncation.

mod common;

use common::{assert_roundtrip, build, Pattern};

/// Pixel byte width exactly 8 is accepted. channels=4, bytes_per_channel=2.
#[test]
fn pixel_bytes_eight_accepted() {
    let image = build(5, 5, 4, 2, Pattern::Random(3));
    assert_roundtrip(&image);
}

/// Widths 5, 6, 7 take the generic template path and round-trip.
#[test]
fn generic_widths_five_six_seven() {
    // 5 = 5x1, 6 = 3x2, 7 = 7x1.
    let cases = [(5u32, 1u32), (3, 2), (7, 1)];
    for &(channels, bpc) in &cases {
        let image = build(6, 4, channels, bpc, Pattern::Gradient);
        assert_roundtrip(&image);
    }
}

/// Zero width or zero height yields a zero-byte image. It still produces a
/// valid blob and round-trips to an empty buffer.
#[test]
fn zero_area() {
    for (w, h) in [(0u32, 5u32), (5, 0), (0, 0)] {
        let image = build(w, h, 3, 1, Pattern::Solid(0));
        assert!(image.buffer.is_empty());
        let blob = zpng::compress(&image).expect("compress empty");
        // Header plus an empty zstd frame.
        assert!(blob.len() >= 8);
        let back = zpng::decompress(&blob).expect("decompress empty");
        assert!(back.buffer.is_empty());
        assert_eq!(back.width_pixels, w);
        assert_eq!(back.height_pixels, h);
    }
}

/// One pixel images for every channel count.
#[test]
fn one_by_one() {
    for channels in 1..=4u32 {
        let image = build(1, 1, channels, 1, Pattern::Random(channels as u64));
        assert_roundtrip(&image);
    }
}

/// Single row and single column exercise the per-row predictor reset.
#[test]
fn single_row_and_column() {
    for channels in 1..=4u32 {
        assert_roundtrip(&build(64, 1, channels, 1, Pattern::Gradient));
        assert_roundtrip(&build(1, 64, channels, 1, Pattern::Gradient));
    }
}

/// Width and height that fill the u16 header fields survive a round trip.
#[test]
fn max_u16_dimension() {
    // A 65535x1 grayscale strip. One channel keeps the buffer small.
    let image = build(65535, 1, 1, 1, Pattern::Stripes);
    assert_roundtrip(&image);
}

/// Dimensions above 65535 truncate to the low 16 bits in the header. This is a
/// format limit, recorded here so the behavior stays stable.
///
/// Compress reads the full geometry to size the pixel buffer, but the header
/// only stores 16 bits of width. The stored width here is 1, so the recorded
/// dimension differs from the real one. The blob no longer round-trips: the
/// frame holds more bytes than the truncated geometry expects, and decompress
/// reports the size mismatch.
#[test]
fn dimension_truncation_is_documented() {
    // width 0x10001 truncates to 1 in the header field.
    let width: u32 = 0x1_0001;
    let mut image = build(1, 1, 1, 1, Pattern::Solid(42));
    image.width_pixels = width;
    image.height_pixels = 1;
    image.buffer = vec![42u8; width as usize]; // buffer matches the real geometry

    let blob = zpng::compress(&image).expect("compress");
    // Stored width is the low 16 bits.
    assert_eq!(u16::from_le_bytes([blob[2], blob[3]]), 1);
    // The truncated header cannot describe the frame, so decode fails.
    assert!(zpng::decompress(&blob).is_none());
}
