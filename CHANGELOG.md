# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.5.0 - 2020-04-10
### Changed
- Re-name `exclude` to `exclude-rename` as it only excludes the module from renaming.
- Updated `sv-parser` to `0.7.0`

### Added
- Add real `exclude` option which excludes specified interfaces, modules and packages
  from being included in the file list.

## 0.4.0 - 2020-04-02
### Changed
- Use `rayon` to parallelize source file parsing.

### Fixed
- Fixed desync of preprocessed text and actual parsing.

### Removed
- Minimization feature.

## 0.3.0 - 2020-03-20
### Added
- Re-add minimization and comment-stripping

### Changed
- Switch to patched `sv-parser` version.
- Switch to `anyhow` result.
- Update dependencies.
- Re-organize uses and mods.

## 0.2.6 - 2020-03-19
### Added
- Build for different Linux distributions

## 0.2.5 - 2020-03-19
### Added
- Publish release artifacts
## Removed
- Legacy Rust CI flow

## 0.2.4 - 2020-03-19
### Fixed
- Clippy suggestions
### Added
- CI infrastructure

## 0.2.3 - 2020-03-15
### Fixed
- Only re-name defined packages and modules.
- Bump `sv-parser` to `0.6.4`.

## 0.2.2 - 2020-03-13
### Fixed
- Re-name modules before they have been declared.

## 0.2.1 - 2020-03-13
### Fixed
- Re-name all package constants (`ClassScope`).

## 0.2.0 - 2020-03-12
### Added
- Add minimzed testcases.
- Add renaming of packages.
- Add interface renaming.
- Add renaming of `endmodule` labels.

### Changed
- Use [sv-parser](https://github.com/dalance/sv-parser) as the main SV parser.

## 0.1.0 - 2019-09-26
### Added
- First version able to re-name modules and instantiations (pre- and suffix).
- Minimization and comment stripping.
- Based on [Moore](https://github.com/fabianschuiki/moore) parser.
