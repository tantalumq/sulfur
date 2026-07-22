# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-07-22

### Added
- 'PackStats', packing statistics (file count, source size, compressed size)
- Packing statistics into the CLI with additional data. (ratio)

### Changed
- 'ArchiveWriter::pack()' now return a tuple '(W, PackStats)'
- Exported 'to_readable_bytes()' to public API

## [0.1.2] - 2026-07-22

### Added
- Keywords and categories

## [0.1.1] - 2026-07-22

### Added
- 'c' / '--compression' flag to CLI to control zstd compression level
- Execution timer for all CLI commands (exclude 'info')
- 'to_readable_bytes()' function to convert raw bytes into a human-readable format.

### Fixed
- Typos in CLI success messages
- Validation for empty directories during extraction
- Error message for file not found in archive

## [0.1.0] - 2026-07-22

### Added
- Initial release
