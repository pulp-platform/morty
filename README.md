![CI](https://github.com/pulp-platform/morty/workflows/CI/badge.svg?branch=master)
[![Crate](https://img.shields.io/crates/v/morty.svg)](https://crates.io/crates/morty)
[![dependency status](https://deps.rs/repo/github/pulp-platform/morty/status.svg)](https://deps.rs/repo/github/pulp-platform/morty)
# Morty

_Come on, flip the pickle, Morty, you're not gonna regret it. The payoff is huge. I turned myself into a pickle, Morty!_

Morty reads SystemVerilog files and pickles them into a single file for easier handling. Optionally it allows to re-name modules with a common prefix or suffix. This allows for easier management of larger projects (they just become a single file). By making them unique they can also depend on different versions of the same dependency without namespace clashes.

## Install

We provide pre-builds for popular operating systems on our [releases page](https://github.com/pulp-platform/morty/releases).

### From Source

Morty is written in Rust. Get the latest stable Rust version:
```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```
Then install `morty` using `cargo`:
```
cargo install --git https://github.com/pulp-platform/morty.git
```

## Example Usage

To prefix all modules and packages in `test/package.sv` and `test/package_import_2.sv` and files with `my_little_prefix_` do:
```
morty test/package.sv test/package_import_2.sv -p my_little_prefix_
```

Alternatively, if you want to pass more files, `morty` will also parse manifest files (as generated by `bender sources -f`). See [Bender](https://github.com/pulp-platform/bender). For example:

```
[
  {
    "include_dirs": [
      "/path/to/include/dir/common_cells/include/",
      "/path/to/include/dir/axi/include/"
    ],
    "defines": {
      "DEFINE_TO_BE_SET": "1"
    },
    "files": [
      "/path/to/file_0.sv",
      "/path/to/file_1.sv",
      "/path/to/file_2.sv"
    ]
  },
  {
    "include_dirs": [
      "/path/to/include/dir/deps/include/"
    ],
    "defines": {
      "ANOTHER_DEFINE_TO_BE_SET": "1"
    },
    "files": [
      "/path/to/file_3.sv",
      "/path/to/file_4.sv",
      "/path/to/file_5.sv"
    ]
  }
]
```

## Comments Stripping

Optionally, `morty` can strip comments (`--strip-comments`) of the pickled sources.

