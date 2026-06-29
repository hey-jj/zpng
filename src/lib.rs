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

use std::fmt;

use header::{Header, HEADER_BYTES};

/// zstd compression level. Higher levels gain little here and cost speed.
const COMPRESSION_LEVEL: i32 = 1;

/// Largest pixel byte width the codec packs. Above this no filter path exists.
const MAX_PIXEL_BYTES: u32 = 8;

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

/// Reason a [`compress`] call failed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompressError {
    /// Pixel byte width, `bytes_per_channel * channels`, exceeds 8. The filter
    /// has no path that wide.
    PixelTooWide {
        /// The computed `bytes_per_channel * channels`.
        pixel_bytes: u32,
    },
    /// The geometry overflows 32-bit pixel math. `width * height * pixel_bytes`
    /// must fit in a `u32`.
    GeometryOverflow,
    /// `buffer` does not hold exactly `width * height * pixel_bytes` bytes.
    BufferSizeMismatch {
        /// Bytes the geometry requires.
        expected: usize,
        /// Bytes the buffer holds.
        got: usize,
    },
}

impl fmt::Display for CompressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompressError::PixelTooWide { pixel_bytes } => {
                write!(f, "pixel byte width {pixel_bytes} exceeds 8")
            }
            CompressError::GeometryOverflow => {
                write!(f, "geometry overflows 32-bit pixel math")
            }
            CompressError::BufferSizeMismatch { expected, got } => {
                write!(f, "buffer holds {got} bytes, geometry needs {expected}")
            }
        }
    }
}

impl std::error::Error for CompressError {}

/// Reason a [`decompress`] call failed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// Blob shorter than the 8-byte header.
    TooShort,
    /// Header magic did not match `0xFBF8`.
    BadMagic,
    /// Header geometry is unusable: pixel byte width above 8, or a pixel count
    /// that overflows 32-bit math.
    BadGeometry,
    /// The zstd frame failed to decode.
    Zstd,
    /// The frame decoded to a size other than the geometry requires.
    SizeMismatch {
        /// Bytes the geometry requires.
        expected: usize,
        /// Bytes the frame produced.
        got: usize,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::TooShort => write!(f, "blob shorter than the 8-byte header"),
            DecodeError::BadMagic => write!(f, "header magic did not match"),
            DecodeError::BadGeometry => write!(f, "header geometry is out of range"),
            DecodeError::Zstd => write!(f, "zstd frame failed to decode"),
            DecodeError::SizeMismatch { expected, got } => {
                write!(f, "frame decoded to {got} bytes, geometry needs {expected}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

/// Compress an image into a blob.
///
/// Returns the blob on success: an 8-byte header followed by a zstd frame.
///
/// # Errors
///
/// - [`CompressError::PixelTooWide`] when `bytes_per_channel * channels`
///   exceeds 8.
/// - [`CompressError::GeometryOverflow`] when `width * height * pixel_bytes`
///   does not fit in a `u32`.
/// - [`CompressError::BufferSizeMismatch`] when `buffer` is not exactly
///   `width * height * pixel_bytes` bytes.
///
/// The header stores width and height as `u16` and channels and
/// bytes-per-channel as `u8`. Values wider than those fields are truncated to
/// the low bits, so dimensions above 65535 do not survive a round trip.
pub fn compress(image: &ImageData) -> Result<Vec<u8>, CompressError> {
    let pixel_bytes = image.bytes_per_channel.wrapping_mul(image.channels);
    if pixel_bytes > MAX_PIXEL_BYTES {
        return Err(CompressError::PixelTooWide { pixel_bytes });
    }

    let byte_count = byte_count(image.width_pixels, image.height_pixels, pixel_bytes)
        .ok_or(CompressError::GeometryOverflow)?;
    if image.buffer.len() != byte_count {
        return Err(CompressError::BufferSizeMismatch {
            expected: byte_count,
            got: image.buffer.len(),
        });
    }

    // Filter the pixels into a scratch buffer. Zero-fill covers the zero-area
    // case where no transform runs.
    let mut packing = vec![0u8; byte_count];
    filter::pack_and_filter(
        &image.buffer,
        &mut packing,
        image.width_pixels as usize,
        image.height_pixels as usize,
        pixel_bytes as usize,
    );

    let frame = zstd::bulk::compress(&packing, COMPRESSION_LEVEL).map_err(|_| {
        // compress_bound sizes the destination, so a real zstd error here means
        // the runtime itself failed. Report it as the wide-pixel guard does not.
        CompressError::GeometryOverflow
    })?;

    let header = Header {
        width: image.width_pixels as u16,
        height: image.height_pixels as u16,
        channels: image.channels as u8,
        bytes_per_channel: image.bytes_per_channel as u8,
    };

    let mut output = Vec::with_capacity(HEADER_BYTES + frame.len());
    output.extend_from_slice(&header.to_bytes());
    output.extend_from_slice(&frame);
    Ok(output)
}

/// Decompress a blob back into an image.
///
/// Returns the reconstructed image on success. The pixel buffer is bit-for-bit
/// equal to the original input.
///
/// `stride_bytes` in the result is `width_pixels * channels`. It omits
/// `bytes_per_channel`, so it is only the true row width when
/// `bytes_per_channel` is 1. The pixel data round-trips correctly regardless.
///
/// # Errors
///
/// - [`DecodeError::TooShort`] when the blob is shorter than 8 bytes.
/// - [`DecodeError::BadMagic`] when the header magic is wrong.
/// - [`DecodeError::BadGeometry`] when the header pixel byte width exceeds 8 or
///   the pixel count overflows 32-bit math.
/// - [`DecodeError::Zstd`] when the frame fails to decode.
/// - [`DecodeError::SizeMismatch`] when the frame decodes to a size other than
///   the geometry requires.
///
/// This decoder is stricter than a frame that only checks for a zstd error. It
/// rejects a frame whose declared or decoded size does not match the header
/// geometry. A frame that under-decodes yields garbage pixels, so rejecting it
/// is the safer default. The check is the reason `decompress` never panics on a
/// crafted header.
pub fn decompress(buffer: &[u8]) -> Result<ImageData, DecodeError> {
    let header = Header::parse(buffer)?;

    let width = header.width as u32;
    let height = header.height as u32;
    let channels = header.channels as u32;
    let bytes_per_channel = header.bytes_per_channel as u32;

    let pixel_bytes = bytes_per_channel * channels;
    if pixel_bytes > MAX_PIXEL_BYTES {
        return Err(DecodeError::BadGeometry);
    }
    // 64-bit math, then range-check. A forged header that would wrap a 32-bit
    // product is rejected here, so the unfilter never indexes past the buffer.
    let byte_count = byte_count(width, height, pixel_bytes).ok_or(DecodeError::BadGeometry)?;

    let frame = &buffer[HEADER_BYTES..];
    // Read the frame's declared content size before allocating. A short blob
    // cannot force a large allocation, and a frame that disagrees with the
    // geometry is rejected without decoding.
    match zstd::zstd_safe::get_frame_content_size(frame) {
        Ok(Some(declared)) if declared == byte_count as u64 => {}
        Ok(Some(declared)) => {
            return Err(DecodeError::SizeMismatch {
                expected: byte_count,
                got: usize::try_from(declared).unwrap_or(usize::MAX),
            });
        }
        // No declared size or an unreadable frame header. Fall through and let
        // the bounded decode below decide.
        _ => {}
    }

    let mut packing = vec![0u8; byte_count];
    let written = zstd::bulk::decompress_to_buffer(frame, &mut packing).map_err(|_| {
        // decompress_to_buffer caps writes at the destination length, so a zstd
        // error means a corrupt or truncated frame.
        DecodeError::Zstd
    })?;
    if written != byte_count {
        return Err(DecodeError::SizeMismatch {
            expected: byte_count,
            got: written,
        });
    }

    let mut output = vec![0u8; byte_count];
    filter::unpack_and_unfilter(
        &packing,
        &mut output,
        width as usize,
        height as usize,
        pixel_bytes as usize,
    );

    Ok(ImageData {
        buffer: output,
        bytes_per_channel,
        channels,
        width_pixels: width,
        height_pixels: height,
        // Mirrors the format quirk: stride drops bytes_per_channel.
        stride_bytes: width.wrapping_mul(channels),
    })
}

/// Total filtered byte count for the geometry, computed in 64-bit math.
///
/// Returns `None` when `width * height * pixel_bytes` does not fit in a `u32`,
/// the range the wire format and the 32-bit reference math allow. Keeping the
/// product inside `u32` means no later index can wrap.
fn byte_count(width: u32, height: u32, pixel_bytes: u32) -> Option<usize> {
    let product = width as u64 * height as u64 * pixel_bytes as u64;
    if product > u32::MAX as u64 {
        return None;
    }
    Some(product as usize)
}
