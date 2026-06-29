//! Lossless image codec. It filters the raw pixel buffer into a more
//! compressible form, then compresses that with zstd. The result is often
//! smaller than PNG and both directions run faster.
//!
//! The pipeline mirrors PNG at a high level. A reversible byte filter runs
//! first, then a general data compressor. The filter subtracts each byte from
//! its left neighbor. For 3- and 4-byte pixels it also decorrelates the color
//! channels and splits them into planes. zstd handles the entropy coding.
//!
//! # Format
//!
//! A compressed blob is an 8-byte header followed by one zstd frame. The header
//! holds the magic value `0xFBF8`, width, height, channel count, and bytes per
//! channel. See [`compress`] for the field widths and truncation rules.
//!
//! # Example
//!
//! ```
//! use zpng::ImageData;
//!
//! // A 2x2 RGB image.
//! let image = ImageData {
//!     buffer: vec![
//!         10, 20, 30, 40, 50, 60, //
//!         70, 80, 90, 100, 110, 120,
//!     ],
//!     bytes_per_channel: 1,
//!     channels: 3,
//!     width_pixels: 2,
//!     height_pixels: 2,
//!     stride_bytes: 6,
//! };
//!
//! let blob = zpng::compress(&image).expect("compress");
//! let back = zpng::decompress(&blob).expect("decompress");
//! assert_eq!(back.buffer, image.buffer);
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod filter;
mod header;

use header::{Header, HEADER_BYTES};

/// zstd compression level. Higher levels gain little here and cost speed.
const COMPRESSION_LEVEL: i32 = 1;

/// A raw image: a tightly packed interleaved pixel buffer plus geometry.
///
/// The buffer holds `width_pixels * height_pixels * channels *
/// bytes_per_channel` bytes in row-major, channel-interleaved order. The codec
/// reads `buffer`, `bytes_per_channel`, `channels`, `width_pixels`, and
/// `height_pixels`. It does not read `stride_bytes`, which the caller sets for
/// its own bookkeeping.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ImageData {
    /// Interleaved pixel bytes.
    pub buffer: Vec<u8>,
    /// Bytes per color channel. Normally 1 or 2.
    pub bytes_per_channel: u32,
    /// Channels per pixel. Normally 1 to 4.
    pub channels: u32,
    /// Image width in pixels.
    pub width_pixels: u32,
    /// Image height in pixels.
    pub height_pixels: u32,
    /// Row width in bytes. Set by the caller, not read during compression.
    pub stride_bytes: u32,
}

/// Compress an image into a blob.
///
/// Returns the blob on success: an 8-byte header followed by a zstd frame.
/// Returns `None` when the pixel byte width exceeds 8, where the pixel byte
/// width is `bytes_per_channel * channels`. That is the only rejected geometry.
///
/// The header stores width and height as `u16` and channels and
/// bytes-per-channel as `u8`. Values wider than those fields are truncated to
/// the low bits, so dimensions above 65535 do not survive a round trip.
///
/// Geometry math uses 32-bit wrapping, matching the byte layout other builds
/// expect. The caller must size `buffer` to the real pixel count.
pub fn compress(image: &ImageData) -> Option<Vec<u8>> {
    let pixel_count = image.width_pixels.wrapping_mul(image.height_pixels);
    let pixel_bytes = image.bytes_per_channel.wrapping_mul(image.channels);
    let byte_count = pixel_bytes.wrapping_mul(pixel_count) as usize;

    // One day this may grow to wider pixels. For now reject them.
    if pixel_bytes > 8 {
        return None;
    }

    // Filter the pixels into a scratch buffer. Zero-fill matches the encoder
    // and covers the zero-area case where no transform runs.
    let mut packing = vec![0u8; byte_count];
    filter::pack_and_filter(
        &image.buffer,
        &mut packing,
        image.width_pixels as usize,
        image.height_pixels as usize,
        pixel_bytes as usize,
    );

    let frame = zstd::bulk::compress(&packing, COMPRESSION_LEVEL).ok()?;

    let header = Header {
        width: image.width_pixels as u16,
        height: image.height_pixels as u16,
        channels: image.channels as u8,
        bytes_per_channel: image.bytes_per_channel as u8,
    };

    let mut output = Vec::with_capacity(HEADER_BYTES + frame.len());
    output.extend_from_slice(&header.to_bytes());
    output.extend_from_slice(&frame);
    Some(output)
}

/// Decompress a blob back into an image.
///
/// Returns the reconstructed image on success. The pixel buffer is bit-for-bit
/// equal to the original input. Returns `None` when the blob is shorter than 8
/// bytes, has the wrong magic, or carries a zstd frame that fails to decode to
/// the expected size.
///
/// `stride_bytes` in the result is `width_pixels * channels`. It omits
/// `bytes_per_channel`, so it is only the true row width when
/// `bytes_per_channel` is 1. The pixel data round-trips correctly regardless.
pub fn decompress(buffer: &[u8]) -> Option<ImageData> {
    let header = Header::parse(buffer)?;

    let width = header.width as u32;
    let height = header.height as u32;
    let channels = header.channels as u32;
    let bytes_per_channel = header.bytes_per_channel as u32;

    let pixel_count = width.wrapping_mul(height);
    let pixel_bytes = bytes_per_channel.wrapping_mul(channels);
    let byte_count = pixel_bytes.wrapping_mul(pixel_count) as usize;

    let frame = &buffer[HEADER_BYTES..];
    let mut packing = vec![0u8; byte_count];
    let written = zstd::bulk::decompress_to_buffer(frame, &mut packing).ok()?;
    if written != byte_count {
        return None;
    }

    let mut output = vec![0u8; byte_count];
    filter::unpack_and_unfilter(
        &packing,
        &mut output,
        width as usize,
        height as usize,
        pixel_bytes as usize,
    );

    Some(ImageData {
        buffer: output,
        bytes_per_channel,
        channels,
        width_pixels: width,
        height_pixels: height,
        // Mirrors the format quirk: stride drops bytes_per_channel.
        stride_bytes: width.wrapping_mul(channels),
    })
}
