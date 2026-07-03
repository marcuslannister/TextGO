# Changelog

All notable changes to TextGO are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Clip extension ingest (Windows): a third-party clip tool (SnipDo) can push selected text into TextGO through an off-by-default, owner-only named-pipe listener, routed through the existing match/execute pipeline as a new `ClipExtension` trigger; ships an installable SnipDo package (`TextGO.pbar`). See [CLIP-EXTENSIONS.md](CLIP-EXTENSIONS.md).

## [0.10.1]

Baseline: first release tracked in this changelog. See the [GitHub release](https://github.com/marcuslannister/TextGO/releases/tag/v0.10.1) for details.

[Unreleased]: https://github.com/marcuslannister/TextGO/compare/v0.10.1...HEAD
[0.10.1]: https://github.com/marcuslannister/TextGO/releases/tag/v0.10.1
