# Synthetic document worker fixtures

Run `python3 tests/knowledge/generate_fixtures.py` to regenerate the checked-in
UTF-8 text, Markdown, PDF and DOCX inputs. They contain invented text only and
use fixed ZIP timestamps. No office suite, external document, network service or
OCR engine is needed. The `inflated.*` fixtures contain about 8 MiB after
inflation but less than 20 KiB on disk; real worker tests deliberately give them
a smaller Rust heap ceiling.

The explicitly registered `document_worker` Cargo target launches
`CARGO_BIN_EXE_vox-bridge`, never the test harness executable. Its Unix tests
exercise the real allocator and framed stdin/stdout protocol. Unit tests use
injected process boundaries for timeout, late-spawn, limit-assignment failure
and unconfirmed-reap cases. Temporary files use the runner's normal `TMPDIR`.

These offline checks do not certify installed application startup, signing,
Windows job assignment under an existing parent job, native CPU/AS enforcement,
or macOS 13 behavior. The Unix allocation limit counts requested Rust heap
bytes and conservative realloc peaks, not whole-process RSS.
