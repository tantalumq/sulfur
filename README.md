# Sulfur
Small file archiver written in Rust

## TODOs
- [x] Main archiver functions (unpack, pack)
- [x] Error handling
- [x] Index array
- [x] Versioning support
- [ ] Cross-platform support #?
- [x] little-endian to big-endian
- [x] Use `clap`
- [x] Unsafe `as` to `::try_into()`
- [x] `info` command
- [x] `get` command (get file from archive by its index)
- [ ] Emerge exit processing
- [ ] Progress bar and logs
- [ ] Configurable
- [ ] Compression flags (`-s` (smart), `-n` (none), `-f` (force))
- [x] Move to lib
- [x] `thiserror`
- [ ] helpers for read_exact
- [ ] Alignment 
- [ ] Concurrency
