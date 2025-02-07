# dupefindr

## Description
A file duplicate detector utility written in Rust

## Author
Written by: Ken Salter

## Command Line Arguments


## NOTE
Do not remove the testdata folder or alter it in any way.  This is used by the tests

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
