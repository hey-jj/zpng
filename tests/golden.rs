//! Golden bitstream locks.
//!
//! Each fixture is a committed `.zpng` blob and its matching raw pixel buffer.
//! Two checks per fixture:
//!
//! 1. Decoding the blob reproduces the raw pixels. This is version-independent
//!    and the primary parity anchor.
//! 2. Re-encoding the raw pixels reproduces the blob byte for byte. This locks
//!    the header plus filtered planes plus the zstd frame. The zstd frame
//!    depends on the linked zstd version, so regenerate these fixtures if that
//!    version changes.
//!
//! Each fixture name encodes its geometry: `<kind>_<w>x<h>`.

use std::fs;
use std::path::PathBuf;

use zpng::ImageData;

fn fixture(name: &str, ext: &str) -> Vec<u8> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures");
    path.push(format!("{name}.{ext}"));
    fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// (name, width, height, channels, bytes_per_channel)
const FIXTURES: &[(&str, u32, u32, u32, u32)] = &[
    ("gray_8x8", 8, 8, 1, 1),
    ("rgb_4x4", 4, 4, 3, 1),
    ("rgba_4x4", 4, 4, 4, 1),
    ("rgb_6x3_random", 6, 3, 3, 1),
];

/// Decoding a golden blob yields the committed raw pixels and the recorded
/// geometry.
#[test]
fn decode_goldens() {
    for &(name, w, h, c, bpc) in FIXTURES {
        let blob = fixture(name, "zpng");
        let raw = fixture(name, "raw");

        let img = zpng::decompress(&blob).unwrap_or_else(|| panic!("decompress {name}"));
        assert_eq!(img.buffer, raw, "pixels for {name}");
        assert_eq!(img.width_pixels, w, "width for {name}");
        assert_eq!(img.height_pixels, h, "height for {name}");
        assert_eq!(img.channels, c, "channels for {name}");
        assert_eq!(img.bytes_per_channel, bpc, "bpc for {name}");
    }
}

/// Re-encoding the raw pixels reproduces the golden blob exactly.
#[test]
fn encode_matches_goldens() {
    for &(name, w, h, c, bpc) in FIXTURES {
        let raw = fixture(name, "raw");
        let blob = fixture(name, "zpng");

        let img = ImageData {
            buffer: raw,
            bytes_per_channel: bpc,
            channels: c,
            width_pixels: w,
            height_pixels: h,
            stride_bytes: w * c * bpc,
        };
        let encoded = zpng::compress(&img).unwrap_or_else(|| panic!("compress {name}"));
        assert_eq!(encoded, blob, "blob bytes for {name}");
    }
}
