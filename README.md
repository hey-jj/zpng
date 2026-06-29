# zpng

Lossless image codec. It filters the raw pixel buffer into a more compressible
form, then compresses that with zstd. The output is often smaller than PNG and
both directions run faster.

The pipeline mirrors PNG at a high level. A reversible byte filter runs first,
then a general data compressor. The filter subtracts each byte from its left
neighbor. For 3- and 4-byte pixels it also decorrelates the color channels and
splits them into planes. zstd handles the entropy coding.

## Install

```toml
[dependencies]
zpng = "0.1"
```

## Use

```rust
use zpng::ImageData;

// A 2x2 RGB image.
let image = ImageData {
    buffer: vec![
        10, 20, 30, 40, 50, 60,
        70, 80, 90, 100, 110, 120,
    ],
    bytes_per_channel: 1,
    channels: 3,
    width_pixels: 2,
    height_pixels: 2,
    stride_bytes: 6,
};

let blob = zpng::compress(&image).expect("compress");
let back = zpng::decompress(&blob).expect("decompress");
assert_eq!(back.buffer, image.buffer);
```

## Format

A compressed blob is an 8-byte header followed by one zstd frame. The header is
little-endian and packed:

| Offset | Size | Field |
|--------|------|-------|
| 0 | 2 | magic `0xFBF8`, bytes `F8 FB` |
| 2 | 2 | width |
| 4 | 2 | height |
| 6 | 1 | channels |
| 7 | 1 | bytes per channel |

Width and height come from 16-bit fields, so they cap at 65535. The pixel byte
width, `channels * bytes_per_channel`, must be 8 or less. Compress returns
`CompressError::PixelTooWide` above that.

## Errors

`compress` returns a `CompressError`: the pixel byte width exceeds 8, the
geometry overflows 32-bit pixel math, or the buffer does not match the geometry.
`decompress` returns a `DecodeError`: the blob is too short, the magic is wrong,
the header geometry is out of range, the zstd frame fails to decode, or the
frame size does not match the geometry. Both types implement `std::error::Error`.

`decompress` validates the header geometry before allocating. A crafted blob
cannot drive a large allocation or an out-of-bounds index.

## License

Licensed under the [MIT license](LICENSE).
