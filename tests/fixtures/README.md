# Golden fixtures

Each pair locks one compressed blob against its raw pixels.

| Name | Width | Height | Channels | Bytes/channel | Pattern |
|------|-------|--------|----------|---------------|---------|
| `gray_8x8` | 8 | 8 | 1 | 1 | gradient |
| `rgb_4x4` | 4 | 4 | 3 | 1 | checker |
| `rgba_4x4` | 4 | 4 | 4 | 1 | stripes |
| `rgb_6x3_random` | 6 | 3 | 3 | 1 | seeded random |

- `*.raw` holds the interleaved pixel bytes.
- `*.zpng` holds the 8-byte header plus one zstd frame.

`golden.rs` checks both directions: decode the blob to the raw pixels, and
re-encode the raw pixels back to the exact blob bytes.

The header and filter bytes are fixed by the format. The zstd frame depends on
the linked zstd version. Regenerate these files if that version changes.
