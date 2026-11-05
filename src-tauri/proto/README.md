# Volcengine AST protobuf source

These four files are unmodified copies of the AST service and its three imports
from the official `protos.tar.gz` attachment in
[`docs/同声传译/火山引擎/doc.md`](../../docs/同声传译/火山引擎/doc.md).
Original paths, packages, field numbers, comments, and event aliases are retained.
Unrelated service files from the archive are not included.

- Source: <https://p9-arcosite.byteimg.com/tos-cn-i-goo7wpa0wc/6493ecf3887246e3a99968961f716499~tplv-goo7wpa0wc-image.image>
- Archive SHA-256: `1e30c61ef9c91d139afd3000f96beeb6ee02f27944f957227ab44c9e986ef576`
- Files: `products/understanding/ast/ast_service.proto`,
  `products/understanding/base/au_base.proto`, `common/events.proto`,
  `common/rpcmeta.proto`.

`build.rs` compiles the AST entry point and imports using `prost-build` and a
vendored `protoc`; no system Protocol Buffers installation is required. The Rust
protocol module references the generated request, response, audio, corpus,
metadata, billing, and event types, so missing required definitions fail the build.

The official schema governs the wire layout when illustrative JSON differs:
`TranslateRequest.source_audio` is top-level field 4, and raw PCM bytes use
`Audio.binary_data` field 14 (`Audio.data` field 1 is a string). Audio packets
contain no WAV header despite the documented StartSession `format="wav"`,
`codec="raw"` settings. Target PCM at 24 kHz is little-endian float32, requested
with `bits=32`; it must be converted to the application's PCM16 playback bus.
`TranslateResponse` has no sentence ID field. The JSON `/ws/ast` gateway shown
elsewhere in the document is a different endpoint from this protobuf protocol.
