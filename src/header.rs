//! The 8-byte blob header.
//!
//! Every compressed blob starts with a fixed header, then a zstd frame. The
//! header stores the geometry needed to rebuild the image. Fields are packed
//! little-endian with no padding, so the header is always exactly
//! [`HEADER_BYTES`] long.

/// Magic value at the start of every blob. On disk the bytes are `F8 FB`.
pub(crate) const MAGIC: u16 = 0xFBF8;

/// Header length in bytes. The zstd frame follows directly after.
pub(crate) const HEADER_BYTES: usize = 8;

/// Parsed blob header.
///
/// Width and height come from `u16` fields, so they top out at 65535. Channels
/// and bytes-per-channel come from `u8` fields. The encoder truncates the
/// caller geometry into these widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Header {
    /// Image width in pixels, low 16 bits of the caller value.
    pub width: u16,
    /// Image height in pixels, low 16 bits of the caller value.
    pub height: u16,
    /// Channel count, low 8 bits of the caller value.
    pub channels: u8,
    /// Bytes per channel, low 8 bits of the caller value.
    pub bytes_per_channel: u8,
}

impl Header {
    /// Serialize the header into 8 bytes.
    ///
    /// Layout: magic (2), width (2), height (2), channels (1),
    /// bytes-per-channel (1). Multi-byte fields are little-endian.
    pub(crate) fn to_bytes(self) -> [u8; HEADER_BYTES] {
        let mut out = [0u8; HEADER_BYTES];
        out[0..2].copy_from_slice(&MAGIC.to_le_bytes());
        out[2..4].copy_from_slice(&self.width.to_le_bytes());
        out[4..6].copy_from_slice(&self.height.to_le_bytes());
        out[6] = self.channels;
        out[7] = self.bytes_per_channel;
        out
    }

    /// Parse a header from the first 8 bytes of a blob.
    ///
    /// Returns `None` when the slice is shorter than [`HEADER_BYTES`] or the
    /// magic does not match.
    pub(crate) fn parse(buffer: &[u8]) -> Option<Header> {
        if buffer.len() < HEADER_BYTES {
            return None;
        }
        let magic = u16::from_le_bytes([buffer[0], buffer[1]]);
        if magic != MAGIC {
            return None;
        }
        Some(Header {
            width: u16::from_le_bytes([buffer[2], buffer[3]]),
            height: u16::from_le_bytes([buffer[4], buffer[5]]),
            channels: buffer[6],
            bytes_per_channel: buffer[7],
        })
    }
}
