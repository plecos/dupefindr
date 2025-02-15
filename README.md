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
| `-0, --include-empty-files` | Include empty files |
| `-H, --include-hidden-files` | Include hidden files |
| `-V, --version` | Print version |
| `-p, --path <PATH>` | The directory to search for duplicates in [default: .] |
| `-q, --quiet` | Hide progress indicators |
| `-r, --recursive` | Recursively search for duplicates |
| `-v, --verbose` | Display verbose output |
| `--create-report` | Create a csv report file |
| `--report-path` | Specify the full path for the report file. Defaults to ./dupefindr-report.csv
| `--debug` | Display debug information |
| `--dry-run` | Dry run the program - This will not delete or modify any files |
| `--exclusion-wildcard <EXCLUSION_WILDCARD>` | Wildcard pattern to exclude. Example: *.txt [default: ] |
| `--help` | Print help |
| `-w, --wildcard <WILDCARD>` | Wildcard pattern to search for. Example: *.txt [default: *] |

# NOTE

Do not remove the testdata folder or alter it in any way. This is used by the tests

## Building from source

### Prerequistes

If you haven't installed Rust before, please install the Rust Development Environment by following the directions here:

[Install Rust](https://www.rust-lang.org/tools/install)

If you see errors during build referring to "linker cc not found", you probably need to install build tools
for your OS.  

[For more information](https://achmadhadikurnia.com/blog/how-to-fix-error-linker-cc-not-found-when-compiling-a-rust-application)

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
cargo test --test-threads=1
```

to run all unit tests.

### NOTE
As of now, test must be run sequentially due to the use of shared instance.  If --test-threads is not used, then tests may fail.

Run

```
cargo tarpaulin
```

to get a report of code coverage
