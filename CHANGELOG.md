# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](http://semver.org/spec/v2.0.0.html).

## Unreleased

## 0.9.0 - 2022-02-15
### Added
- Add switch to disable parallel file parsing (can help with STACKOVERFLOW)
- Add switch to propagate defines from parsed files to subsequent files
- Add support for flist files, capable of parsing a list of files with `+incdir+` and `+define+` arguments.

### Fixed
- correct global package parsing

### Changed
- remove defines from pickled output, with flag to keep
- remove `timeunit` and `timeprecision` from pickle, with flag to keep

## 0.8.0 - 2022-08-26
### Added
- Add `Compiled by morty` comment to output files (with version and datetime)
- `top_module` parameter to restrict pickled output and output manifest to the needed files for module hierarchy
- Add `-i` flag to ignore unparseable files

## 0.7.0 - 2022-08-09
### Fixed
- Update `clap` to `v3`
- Remove `failure` dependency
- Add compatibility for new `bender` formats
- Correctly parse model identifiers with `(* attribute *)`
- Update `sv-parser` to `0.12`
- Update `simple_logger` to `2.2`
- Update `pulldown-cmark` to `0.9`

### Changed
- Change crate to allow use of functionality with a Rust library

## 0.6.0 - 2022-01-21
### Fixed
- Bump `svparser` to `0.11.1`
- Use builder pattern for simple logger
- Add readable errors instead of `unwrap`

### Added
- Add `-o` flag for fiile output
- List undefined modules if detected
- Support loading library files
- Add option to output manifest

## 0.5.2 - 2021-02-04
### Fixed
- Fixed deprecated `add-path` in CI

## 0.5.1 - 2021-02-04
### Fixed
- Ignore comments starting with four slashes in documentation.
- Update `sv-parser` to `0.10.8`
- Update `pulldown-cmark` to `0.8.0`
- Update `failure` to `0.1.8`
- Update `colored` to `2.0.0`

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
