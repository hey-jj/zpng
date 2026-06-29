//! Golden bitstream locks.
//!
//! Each fixture is a committed `.zpng` blob and its matching raw pixel buffer.
//! Three checks per fixture:
//!
//! 1. Decoding the blob reproduces the raw pixels. This is version-independent
//!    and the primary parity anchor.
//! 2. The header bytes and the filtered planes match the algorithm. These are
//!    version-independent. They pin the wire format that any consumer relies on.
//! 3. Re-encoding the raw pixels reproduces the blob byte for byte. This lock is
//!    self-referential: the committed frames came from the same linked zstd that
//!    the test re-encodes with, so it proves the encoder is stable against
//!    itself, not that the bytes match a fixed external reference. zstd frame
//!    bytes vary across zstd versions, so regenerate these fixtures when the
//!    linked zstd changes.
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
/// geometry. Version-independent.
#[test]
fn decode_goldens() {
    for &(name, w, h, c, bpc) in FIXTURES {
        let blob = fixture(name, "zpng");
        let raw = fixture(name, "raw");

        let img = zpng::decompress(&blob).unwrap_or_else(|e| panic!("decompress {name}: {e}"));
        assert_eq!(img.buffer, raw, "pixels for {name}");
        assert_eq!(img.width_pixels, w, "width for {name}");
        assert_eq!(img.height_pixels, h, "height for {name}");
        assert_eq!(img.channels, c, "channels for {name}");
        assert_eq!(img.bytes_per_channel, bpc, "bpc for {name}");
    }
}

/// The header bytes and the filtered planes match the algorithm. Both are
/// version-independent: the header is fixed by the format, and the planes are
/// derived from the raw pixels by a reference filter in this file, then
/// compared against the committed frame after a plain zstd decode. This is the
/// part of the lock that does not depend on the linked zstd version.
#[test]
fn header_and_planes_match() {
    for &(name, w, h, c, bpc) in FIXTURES {
        let blob = fixture(name, "zpng");
        let raw = fixture(name, "raw");

        // Header is exactly 8 little-endian bytes.
        let mut expected_header = vec![0xF8u8, 0xFB];
        expected_header.extend_from_slice(&(w as u16).to_le_bytes());
        expected_header.extend_from_slice(&(h as u16).to_le_bytes());
        expected_header.push(c as u8);
        expected_header.push(bpc as u8);
        assert_eq!(&blob[0..8], &expected_header[..], "header for {name}");

        // Decode the committed frame and compare against the reference filter.
        let frame = &blob[8..];
        let pixel_bytes = (c * bpc) as usize;
        let byte_count = w as usize * h as usize * pixel_bytes;
        let committed_planes = zstd::bulk::decompress(frame, byte_count)
            .unwrap_or_else(|e| panic!("zstd decode {name}: {e}"));
        let expected_planes = reference_filter(&raw, w as usize, h as usize, pixel_bytes);
        assert_eq!(
            committed_planes, expected_planes,
            "filtered planes for {name}"
        );
    }
}

/// Re-encoding the raw pixels reproduces the golden blob exactly. This lock is
/// self-referential. The committed frames came from the same linked zstd this
/// test re-encodes with, so it proves the encoder is stable against itself, not
/// that the bytes match a fixed external reference. zstd frame bytes vary by
/// version. Regenerate the fixtures when the linked zstd changes.
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
        let encoded = zpng::compress(&img).unwrap_or_else(|e| panic!("compress {name}: {e}"));
        assert_eq!(encoded, blob, "blob bytes for {name}");
    }
}

/// Plain transcription of the filter, independent of the crate internals. The
/// 3- and 4-byte paths apply the GB-RG color transform and split into planes.
/// Every other width is a per-byte left delta over the interleaved buffer. Used
/// to pin the committed planes without exposing the private filter.
fn reference_filter(input: &[u8], width: usize, height: usize, pixel_bytes: usize) -> Vec<u8> {
    let mut output = vec![0u8; input.len()];
    match pixel_bytes {
        3 | 4 => {
            let plane_bytes = width * height;
            let mut in_pos = 0;
            let mut p = 0;
            for _row in 0..height {
                let mut prev = [0u8; 4];
                for _x in 0..width {
                    let r = input[in_pos];
                    let g = input[in_pos + 1];
                    let b = input[in_pos + 2];
                    let dr = r.wrapping_sub(prev[0]);
                    let dg = g.wrapping_sub(prev[1]);
                    let db = b.wrapping_sub(prev[2]);
                    prev[0] = r;
                    prev[1] = g;
                    prev[2] = b;
                    output[p] = db;
                    output[plane_bytes + p] = dg.wrapping_sub(db);
                    output[plane_bytes * 2 + p] = dg.wrapping_sub(dr);
                    if pixel_bytes == 4 {
                        let a = input[in_pos + 3];
                        output[plane_bytes * 3 + p] = a.wrapping_sub(prev[3]);
                        prev[3] = a;
                    }
                    in_pos += pixel_bytes;
                    p += 1;
                }
            }
        }
        _ => {
            let mut pos = 0;
            for _y in 0..height {
                let mut prev = [0u8; 8];
                for _x in 0..width {
                    for i in 0..pixel_bytes {
                        let a = input[pos + i];
                        output[pos + i] = a.wrapping_sub(prev[i]);
                        prev[i] = a;
                    }
                    pos += pixel_bytes;
                }
            }
        }
    }
    output
}
