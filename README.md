# dupefindr

## Description

A file duplicate detector utility written in Rust

## Author

Written by: Ken Salter

## Command Line Arguments

Usage: dupefindr [OPTIONS] <COMMAND>

Commands:
- `find`    - Find duplicate files
- `move`    - Move duplicate files to a new location
- `copy`    - Copy duplicate files to a new location
- `delete`  - Delete duplicate files
- `help`    - Print this message or the help of the given subcommand(s)

Options:

| Option | Description |
|--------|-------------|
| `-p, --path <PATH>` | The directory to search for duplicates in [default: .] |
| `-w, --wildcard <WILDCARD>` | Wildcard pattern to search for. Example: *.txt [default: *] |
| `--exclusion-wildcard <EXCLUSION_WILDCARD>` | Wildcard pattern to exclude. Example: *.txt [default: ] |
| `-r, --recursive` | Recursively search for duplicates |
| `--debug` | Display debug information |
| `-0, --include-empty-files` | Include empty files |
| `--dry-run` | Dry run the program - This will not delete or modify any files |
| `-H, --include-hidden-files` | Include hidden files |
| `-q, --quiet` | Hide progress indicators |
| `-v, --verbose` | Display verbose output |
| `--help` | Print help |
| `-V, --version` | Print version |

# NOTE

Do not remove the testdata folder or alter it in any way. This is used by the tests

## Building from source

### Prerequistes

If you haven't installed Rust before, please install the Rust Development Environment by following the directions here:

[Install Rust](https://www.rust-lang.org/tools/install)

### Release

To build the production release:

```
cargo build --release
```

The executable will be located in \target\release

### Debug

To build the debug release:

```
cargo build
```

The executable will be located in \target\debug

## Testing

Run

```
cargo test
```

to run all unit tests.

Run

```
cargo tarpaulin
```

to get a report of code coverage
