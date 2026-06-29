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

`golden.rs` runs three checks. Decode the blob to the raw pixels. Match the
header bytes and the filtered planes against the algorithm. Re-encode the raw
pixels back to the exact blob bytes.

The header and the filtered planes are fixed by the format and do not depend on
zstd. They are the parity anchor. The whole-blob re-encode check is
self-referential: the committed frames came from the same linked zstd that the
test re-encodes with, so it locks the encoder against itself, not against a
fixed external reference. zstd frame bytes vary by version, so regenerate these
files when the linked zstd changes.
