//! Shared helpers for the integration tests.
//!
//! These build deterministic synthetic images so the suite needs no external
//! files. Every builder fills the pixel buffer to the exact size the codec
//! expects and sets `stride_bytes` to `width * channels * bytes_per_channel`.

#![allow(dead_code)]

use zpng::ImageData;

/// Pixel fill patterns used across the tests.
#[derive(Clone, Copy, Debug)]
pub enum Pattern {
    /// Every byte set to the given value.
    Solid(u8),
    /// `byte = (x + y + channel_index) as u8`. Exercises the delta filter and
    /// the mod-256 wrap.
    Gradient,
    /// Vertical stripes that flip every other column.
    Stripes,
    /// Pseudo-random bytes from a seeded xorshift generator.
    Random(u64),
    /// Checkerboard alternating two values per cell.
    Checker,
}

/// Build an image with the given geometry and fill pattern.
///
/// `bytes_per_channel` channels are treated as a flat run of bytes per pixel,
/// matching how the codec walks the buffer.
pub fn build(
    width: u32,
    height: u32,
    channels: u32,
    bytes_per_channel: u32,
    pattern: Pattern,
) -> ImageData {
    let pixel_bytes = (channels * bytes_per_channel) as usize;
    let len = width as usize * height as usize * pixel_bytes;
    let mut buffer = vec![0u8; len];

    let mut rng = Xorshift::new(match pattern {
        Pattern::Random(seed) => seed.max(1),
        _ => 1,
    });

    let mut idx = 0;
    for y in 0..height as usize {
        for x in 0..width as usize {
            for c in 0..pixel_bytes {
                let value = match pattern {
                    Pattern::Solid(v) => v,
                    Pattern::Gradient => (x + y + c) as u8,
                    Pattern::Stripes => {
                        if x % 2 == 0 {
                            0x20
                        } else {
                            0xE0
                        }
                    }
                    Pattern::Random(_) => rng.next_u8(),
                    Pattern::Checker => {
                        if (x + y) % 2 == 0 {
                            0x10u8.wrapping_add(c as u8)
                        } else {
                            0xF0u8.wrapping_sub(c as u8)
                        }
                    }
                };
                buffer[idx] = value;
                idx += 1;
            }
        }
    }

    ImageData {
        buffer,
        bytes_per_channel,
        channels,
        width_pixels: width,
        height_pixels: height,
        stride_bytes: width * channels * bytes_per_channel,
    }
}

/// Assert that compress then decompress reproduces the image exactly and
/// reports the expected metadata.
pub fn assert_roundtrip(image: &ImageData) {
    let blob = zpng::compress(image).expect("compress returned None");
    let back = zpng::decompress(&blob).expect("decompress returned None");

    assert_eq!(back.bytes_per_channel, image.bytes_per_channel);
    assert_eq!(back.channels, image.channels);
    assert_eq!(back.height_pixels, image.height_pixels);
    assert_eq!(back.width_pixels, image.width_pixels);
    // Stride is the tightly packed row width.
    assert_eq!(
        back.stride_bytes,
        image.width_pixels * image.channels * image.bytes_per_channel
    );
    assert_eq!(back.buffer.len(), image.buffer.len());
    assert_eq!(back.buffer, image.buffer);
}

/// Small xorshift64 generator. Deterministic and dependency-free.
pub struct Xorshift {
    state: u64,
}

impl Xorshift {
    pub fn new(seed: u64) -> Self {
        Xorshift { state: seed.max(1) }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    pub fn next_u8(&mut self) -> u8 {
        (self.next_u64() & 0xFF) as u8
    }
}
