//! Reversible byte filter that runs before zstd.
//!
//! The filter makes pixel data more compressible without losing any bits. It
//! has three jobs:
//!
//! 1. Subtract each byte from its left neighbor in the same row. The predictor
//!    resets to zero at the start of every row.
//! 2. For 3- and 4-byte pixels, decorrelate the color channels with the GB-RG
//!    transform from BCIF.
//! 3. For 3- and 4-byte pixels, split the channels into separate planes.
//!
//! All arithmetic is 8-bit and wraps mod 256, so the transform is exactly
//! reversible. Dispatch keys on `pixel_bytes`, the byte width of one pixel,
//! not on the semantic channel count. A 16-bit grayscale pixel
//! (`bytes_per_channel = 2`, `channels = 1`) has `pixel_bytes = 2` and takes
//! the generic two-byte path. A 16-bit two-channel pixel has `pixel_bytes = 4`
//! and takes the color path, same as RGBA. This matches the encoder dispatch
//! and keeps the filtered bytes, and therefore the compressed output,
//! identical across builds.

/// Apply the filter to `input`, writing `input.len()` bytes into `output`.
///
/// `width` and `height` are pixel counts. `pixel_bytes` is the byte width of
/// one pixel and selects the code path. When `pixel_bytes` is 0 or above 8 no
/// transform runs and `output` keeps whatever it held, matching the encoder
/// switch that has no default case.
///
/// `input` and `output` must both be `width * height * pixel_bytes` bytes.
pub(crate) fn pack_and_filter(
    input: &[u8],
    output: &mut [u8],
    width: usize,
    height: usize,
    pixel_bytes: usize,
) {
    match pixel_bytes {
        3 => pack_color::<3>(input, output, width, height),
        4 => pack_color::<4>(input, output, width, height),
        1 | 2 | 5 | 6 | 7 | 8 => pack_generic(input, output, width, height, pixel_bytes),
        _ => {}
    }
}

/// Invert the filter, reconstructing the original pixel bytes.
///
/// `input` holds the filtered bytes produced by [`pack_and_filter`]. `output`
/// receives the reconstructed pixels. Sizes and the `pixel_bytes` dispatch
/// match the forward direction.
pub(crate) fn unpack_and_unfilter(
    input: &[u8],
    output: &mut [u8],
    width: usize,
    height: usize,
    pixel_bytes: usize,
) {
    match pixel_bytes {
        3 => unpack_color::<3>(input, output, width, height),
        4 => unpack_color::<4>(input, output, width, height),
        1 | 2 | 5 | 6 | 7 | 8 => unpack_generic(input, output, width, height, pixel_bytes),
        _ => {}
    }
}

/// Per-byte left-delta over the interleaved buffer. No color transform, no
/// plane split. Used for every pixel width except 3 and 4.
fn pack_generic(input: &[u8], output: &mut [u8], width: usize, height: usize, channels: usize) {
    let mut pos = 0;
    for _y in 0..height {
        let mut prev = [0u8; 8];
        for _x in 0..width {
            for i in 0..channels {
                let a = input[pos + i];
                output[pos + i] = a.wrapping_sub(prev[i]);
                prev[i] = a;
            }
            pos += channels;
        }
    }
}

/// Exact inverse of [`pack_generic`].
fn unpack_generic(input: &[u8], output: &mut [u8], width: usize, height: usize, channels: usize) {
    let mut pos = 0;
    for _y in 0..height {
        let mut prev = [0u8; 8];
        for _x in 0..width {
            for i in 0..channels {
                let a = input[pos + i].wrapping_add(prev[i]);
                output[pos + i] = a;
                prev[i] = a;
            }
            pos += channels;
        }
    }
}

/// Color path for 3- and 4-byte pixels.
///
/// Each pixel is left-deltaed per channel, then the first three channels run
/// through the GB-RG transform. A fourth channel, when present, is delta-only.
/// Results land in separate planes: Y, U, V, then A. Each plane is
/// `width * height` bytes.
///
/// `N` is the pixel width, 3 or 4.
fn pack_color<const N: usize>(input: &[u8], output: &mut [u8], width: usize, height: usize) {
    let plane_bytes = width * height;
    let mut in_pos = 0;
    // Running write index into each output plane.
    let mut p = 0;
    for _row in 0..height {
        let mut prev = [0u8; N];
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

            // GB-RG filter from BCIF.
            output[p] = db; // Y plane
            output[plane_bytes + p] = dg.wrapping_sub(db); // U plane
            output[plane_bytes * 2 + p] = dg.wrapping_sub(dr); // V plane

            if N == 4 {
                let a = input[in_pos + 3];
                output[plane_bytes * 3 + p] = a.wrapping_sub(prev[3]); // A plane, delta only
                prev[3] = a;
            }

            in_pos += N;
            p += 1;
        }
    }
}

/// Exact inverse of [`pack_color`].
fn unpack_color<const N: usize>(input: &[u8], output: &mut [u8], width: usize, height: usize) {
    let plane_bytes = width * height;
    let mut out_pos = 0;
    let mut p = 0;
    for _row in 0..height {
        let mut prev = [0u8; N];
        for _x in 0..width {
            let y = input[p];
            let u = input[plane_bytes + p];
            let v = input[plane_bytes * 2 + p];

            // Inverse GB-RG filter.
            let big_b = y;
            let big_g = u.wrapping_add(big_b);
            let dr = big_g.wrapping_sub(v);
            let dg = big_g;
            let db = big_b;

            let r = dr.wrapping_add(prev[0]);
            let g = dg.wrapping_add(prev[1]);
            let b = db.wrapping_add(prev[2]);

            output[out_pos] = r;
            output[out_pos + 1] = g;
            output[out_pos + 2] = b;

            prev[0] = r;
            prev[1] = g;
            prev[2] = b;

            if N == 4 {
                let a = input[plane_bytes * 3 + p].wrapping_add(prev[3]);
                output[out_pos + 3] = a;
                prev[3] = a;
            }

            out_pos += N;
            p += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-computed RGB example. One row of two pixels.
    ///
    /// Input pixels `(10,20,30)` and `(15,28,33)`. The predictor resets to zero
    /// at row start.
    ///
    /// Pixel 0: dr=10, dg=20, db=30. y=db=30, u=dg-db=20-30=246, v=dg-dr=20-10=10.
    /// Pixel 1: dr=15-10=5, dg=28-20=8, db=33-30=3. y=3, u=8-3=5, v=8-5=3.
    /// Planes: Y=[30,3], U=[246,5], V=[10,3].
    #[test]
    fn rgb_hand_computed() {
        let input = [10u8, 20, 30, 15, 28, 33];
        let mut output = [0u8; 6];
        pack_and_filter(&input, &mut output, 2, 1, 3);
        assert_eq!(output, [30, 3, 246, 5, 10, 3]);

        let mut back = [0u8; 6];
        unpack_and_unfilter(&output, &mut back, 2, 1, 3);
        assert_eq!(back, input);
    }

    /// Grayscale left-delta over one row. Predictor resets each row.
    #[test]
    fn gray_hand_computed() {
        let input = [10u8, 13, 13, 200];
        let mut output = [0u8; 4];
        pack_and_filter(&input, &mut output, 4, 1, 1);
        // 10-0=10, 13-10=3, 13-13=0, 200-13=187.
        assert_eq!(output, [10, 3, 0, 187]);

        let mut back = [0u8; 4];
        unpack_and_unfilter(&output, &mut back, 4, 1, 1);
        assert_eq!(back, input);
    }

    /// Mod-256 wrap. The delta underflows then re-adds on the way back.
    #[test]
    fn wrap_underflow() {
        let input = [0u8, 200, 5];
        let mut output = [0u8; 3];
        pack_and_filter(&input, &mut output, 3, 1, 1);
        // 0-0=0, 200-0=200, 5-200=61 (wrap).
        assert_eq!(output, [0, 200, 61]);

        let mut back = [0u8; 3];
        unpack_and_unfilter(&output, &mut back, 3, 1, 1);
        assert_eq!(back, input);
    }

    /// RGBA. The alpha plane is delta-only, no color transform. Plane order is
    /// Y, U, V, A, each `width*height` bytes.
    #[test]
    fn rgba_plane_order() {
        // One row, one pixel: r=40, g=50, b=60, a=70.
        let input = [40u8, 50, 60, 70];
        let mut output = [0u8; 4];
        pack_and_filter(&input, &mut output, 1, 1, 4);
        // dr=40, dg=50, db=60, da=70. y=60, u=50-60=246, v=50-40=10, a=70.
        assert_eq!(output, [60, 246, 10, 70]);

        let mut back = [0u8; 4];
        unpack_and_unfilter(&output, &mut back, 1, 1, 4);
        assert_eq!(back, input);
    }

    /// Per-row predictor reset. Two rows of one RGB pixel each: the second row
    /// must filter against zero, not against the first row.
    #[test]
    fn row_reset() {
        let input = [10u8, 20, 30, 100, 110, 120];
        let mut output = [0u8; 6];
        pack_and_filter(&input, &mut output, 1, 2, 3);
        // Row 0 pixel: dr=10,dg=20,db=30 -> y=30,u=246,v=10.
        // Row 1 pixel: dr=100,dg=110,db=120 -> y=120,u=110-120=246,v=110-100=10.
        // Planes Y=[30,120], U=[246,246], V=[10,10].
        assert_eq!(output, [30, 120, 246, 246, 10, 10]);

        let mut back = [0u8; 6];
        unpack_and_unfilter(&output, &mut back, 1, 2, 3);
        assert_eq!(back, input);
    }

    /// Generic five-byte path. Interleaved delta, no plane split.
    #[test]
    fn generic_five_byte() {
        // One row, two pixels of 5 bytes each.
        let input = [1u8, 2, 3, 4, 5, 11, 13, 15, 17, 19];
        let mut output = [0u8; 10];
        pack_and_filter(&input, &mut output, 2, 1, 5);
        // Pixel 0: each minus 0. Pixel 1: 11-1,13-2,15-3,17-4,19-5.
        assert_eq!(output, [1, 2, 3, 4, 5, 10, 11, 12, 13, 14]);

        let mut back = [0u8; 10];
        unpack_and_unfilter(&output, &mut back, 2, 1, 5);
        assert_eq!(back, input);
    }

    /// A 2-channel 16-bit image has pixel_bytes 4 and takes the color path, the
    /// same path RGBA takes. Dispatch keys on the byte width, not the channel
    /// count, so the color transform applies to its bytes. This pins the
    /// filtered layout for that shape.
    ///
    /// Input 2x1, pixel_bytes 4, bytes `[10,20,30,40, 50,60,70,80]`.
    /// Pixel 0: dr=10,dg=20,db=30,da=40. Y=30, U=20-30=246, V=20-10=10, A=40.
    /// Pixel 1: dr=40,dg=40,db=40,da=40. Y=40, U=0, V=0, A=40.
    /// Planes Y=[30,40] U=[246,0] V=[10,0] A=[40,40].
    #[test]
    fn color_path_via_two_channel_16bit() {
        let input = [10u8, 20, 30, 40, 50, 60, 70, 80];
        let mut output = [0u8; 8];
        pack_and_filter(&input, &mut output, 2, 1, 4);
        assert_eq!(output, [30, 40, 246, 0, 10, 0, 40, 40]);

        let mut back = [0u8; 8];
        unpack_and_unfilter(&output, &mut back, 2, 1, 4);
        assert_eq!(back, input);
    }

    /// Generic paths for pixel_bytes 2, 6, 7, 8. Per-byte left delta, predictor
    /// reset per row, interleaved output. Each case is hand-computed from the
    /// algorithm so the byte layout is pinned, not just invertibility.
    #[test]
    fn generic_paths_exact_bytes() {
        // (pixel_bytes, width, height, input, expected_filtered)
        struct Case {
            pixel_bytes: usize,
            width: usize,
            height: usize,
            input: &'static [u8],
            filtered: &'static [u8],
        }
        let cases = [
            Case {
                pixel_bytes: 2,
                width: 3,
                height: 1,
                input: &[10, 200, 5, 250, 1, 2],
                // [10-0,200-0, 5-10,250-200, 1-5,2-250]
                filtered: &[10, 200, 251, 50, 252, 8],
            },
            Case {
                pixel_bytes: 6,
                width: 2,
                height: 1,
                input: &[1, 2, 3, 4, 5, 6, 100, 101, 102, 103, 104, 105],
                filtered: &[1, 2, 3, 4, 5, 6, 99, 99, 99, 99, 99, 99],
            },
            Case {
                pixel_bytes: 7,
                width: 2,
                height: 1,
                input: &[0, 1, 2, 3, 4, 5, 6, 250, 251, 252, 253, 254, 255, 0],
                // second pixel each byte minus the first, last wraps: 0-6=250
                filtered: &[0, 1, 2, 3, 4, 5, 6, 250, 250, 250, 250, 250, 250, 250],
            },
            Case {
                pixel_bytes: 8,
                width: 2,
                height: 1,
                input: &[
                    10, 20, 30, 40, 50, 60, 70, 80, 5, 25, 35, 45, 55, 65, 75, 85,
                ],
                // 5-10 wraps to 251, the rest are constant +5 deltas
                filtered: &[10, 20, 30, 40, 50, 60, 70, 80, 251, 5, 5, 5, 5, 5, 5, 5],
            },
        ];

        for case in cases {
            let mut output = vec![0u8; case.input.len()];
            pack_and_filter(
                case.input,
                &mut output,
                case.width,
                case.height,
                case.pixel_bytes,
            );
            assert_eq!(
                output, case.filtered,
                "filtered bytes for pixel_bytes {}",
                case.pixel_bytes
            );

            let mut back = vec![0u8; case.input.len()];
            unpack_and_unfilter(
                &output,
                &mut back,
                case.width,
                case.height,
                case.pixel_bytes,
            );
            assert_eq!(
                back, case.input,
                "inverse for pixel_bytes {}",
                case.pixel_bytes
            );
        }
    }
}
