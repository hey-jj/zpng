//! Degenerate and boundary geometries. Zero-area images, single rows and
//! columns, the maximum accepted pixel width, and header field overflow.

mod common;

use common::{assert_roundtrip, build, Pattern};
use zpng::{CompressError, ImageData};

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

/// Pixel byte width zero. Either factor at zero gives `pixel_bytes == 0`, which
/// is not the wide-pixel reject case, so compress proceeds. No filter runs and
/// zstd compresses an empty input. The header still records the zero field.
#[test]
fn pixel_bytes_zero() {
    // channels = 0.
    let image = ImageData {
        buffer: Vec::new(),
        bytes_per_channel: 1,
        channels: 0,
        width_pixels: 4,
        height_pixels: 4,
        stride_bytes: 0,
    };
    let blob = zpng::compress(&image).expect("compress channels=0");
    assert_eq!(blob[6], 0, "channels byte is zero");
    let back = zpng::decompress(&blob).expect("decompress channels=0");
    assert!(back.buffer.is_empty());
    assert_eq!(back.channels, 0);
    assert_eq!(back.width_pixels, 4);
    assert_eq!(back.height_pixels, 4);

    // bytes_per_channel = 0.
    let image = ImageData {
        buffer: Vec::new(),
        bytes_per_channel: 0,
        channels: 3,
        width_pixels: 4,
        height_pixels: 4,
        stride_bytes: 0,
    };
    let blob = zpng::compress(&image).expect("compress bpc=0");
    assert_eq!(blob[7], 0, "bytes-per-channel byte is zero");
    let back = zpng::decompress(&blob).expect("decompress bpc=0");
    assert!(back.buffer.is_empty());
    assert_eq!(back.bytes_per_channel, 0);
    assert_eq!(back.channels, 3);
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

/// A width above 65535 does not fit the u16 header field, so compress rejects it
/// rather than truncating to the low 16 bits and emitting a blob that cannot
/// round trip. The buffer matches the real geometry, so the only failure is the
/// field overflow.
#[test]
fn dimension_above_u16_is_rejected() {
    let width: u32 = 0x1_0001;
    let mut image = build(1, 1, 1, 1, Pattern::Solid(42));
    image.width_pixels = width;
    image.height_pixels = 1;
    image.buffer = vec![42u8; width as usize];

    assert_eq!(
        zpng::compress(&image),
        Err(CompressError::HeaderFieldOverflow)
    );
}
