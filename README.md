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
- [ ] `info` command
- [x] `get` command (get file from archive by its index)
- [ ] Emerge exit processing
- [ ] Progress bar and logs
- [ ] Configurable
- [ ] Smart compression (flags: `-s` (smart), `-n` (none), `-f` (force))
- [x] Move to lib
- [x] `thiserror`
- [ ] remove setters for innerfile structure 
- [ ] helpers for read_exact
- [ ] Alignment 
- [ ] Concurrency
