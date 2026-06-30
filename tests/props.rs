//! Property tests. Random images of any accepted shape round-trip exactly and
//! report matching geometry. The blob always starts with the right header.

mod common;

use common::{build, Pattern};
use proptest::prelude::*;
use zpng::ImageData;

proptest! {
    /// Any image with `channels * bytes_per_channel <= 8` survives a round trip
    /// with identical bytes and metadata.
    #[test]
    fn roundtrip_any(
        w in 1u32..=48,
        h in 1u32..=48,
        channels in 1u32..=4,
        bytes_per_channel in 1u32..=2,
        seed in any::<u64>(),
    ) {
        prop_assume!(channels * bytes_per_channel <= 8);
        let image = build(w, h, channels, bytes_per_channel, Pattern::Random(seed | 1));

        let blob = zpng::compress(&image).expect("compress");
        let back = zpng::decompress(&blob).expect("decompress");

        prop_assert_eq!(&back.buffer, &image.buffer);
        prop_assert_eq!(back.width_pixels, w);
        prop_assert_eq!(back.height_pixels, h);
        prop_assert_eq!(back.channels, channels);
        prop_assert_eq!(back.bytes_per_channel, bytes_per_channel);
        prop_assert_eq!(back.stride_bytes, w * channels * bytes_per_channel);
    }

    /// Compress output always starts with the correct little-endian header for
    /// arbitrary valid shapes.
    #[test]
    fn header_prefix_correct(
        w in 1u32..=300,
        h in 1u32..=300,
        channels in 1u32..=4,
        bytes_per_channel in 1u32..=2,
    ) {
        prop_assume!(channels * bytes_per_channel <= 8);
        let image = build(w, h, channels, bytes_per_channel, Pattern::Solid(0));
        let blob = zpng::compress(&image).expect("compress");

        prop_assert_eq!(&blob[0..2], &[0xF8, 0xFB]);
        prop_assert_eq!(u16::from_le_bytes([blob[2], blob[3]]), w as u16);
        prop_assert_eq!(u16::from_le_bytes([blob[4], blob[5]]), h as u16);
        prop_assert_eq!(blob[6], channels as u8);
        prop_assert_eq!(blob[7], bytes_per_channel as u8);
    }

    /// Decompress is the left inverse of compress. Two passes give the same
    /// image, so compress is deterministic and decompress is stable.
    #[test]
    fn decompress_is_stable(
        w in 1u32..=32,
        h in 1u32..=32,
        channels in 1u32..=4,
        seed in any::<u64>(),
    ) {
        let image: ImageData = build(w, h, channels, 1, Pattern::Random(seed | 1));
        let blob_a = zpng::compress(&image).expect("compress");
        let blob_b = zpng::compress(&image).expect("compress");
        prop_assert_eq!(&blob_a, &blob_b);

        let back = zpng::decompress(&blob_a).expect("decompress");
        prop_assert_eq!(back.buffer, image.buffer);
    }
}
