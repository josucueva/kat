# Installation

The `kat` CLI is a single Rust binary that builds on **Linux** and **Windows**.

## Prerequisites

- [Rust](https://rustup.rs) (stable, via `rustup`)
- Git (to clone the repository)

## Install

From the repository root:

```bash
cargo install --path .
```

This builds in release mode and installs `kat` into Cargo's bin directory:

- **Linux:** `~/.cargo/bin/kat`
- **Windows:** `%USERPROFILE%\.cargo\bin\kat.exe`

That directory is on `PATH` by default when Rust is installed via `rustup`, so
`kat` becomes available from anywhere.

## Manual build (no install)

```bash
cargo build --release
./target/release/kat          # Linux
.\target\release\kat.exe      # Windows
```

## Platform notes

### Linux

The native GNU toolchain is the default; no extra setup is required.

### Windows

- **With VS Build Tools (MSVC):** the default toolchain
  (`x86_64-pc-windows-msvc`) produces a self-contained `kat.exe`.
- **Without (MinGW/GNU):** set the machine-local override and keep the MinGW
  runtime on `PATH`:

  ```powershell
  rustup override set stable-x86_64-pc-windows-gnu
  $env:PATH = "$env:USERPROFILE\.cargo\bin;C:\msys64\mingw64\bin;$env:PATH"
  cargo install --path .
  ```

  The GNU-built binary depends on MinGW runtime DLLs, so `C:\msys64\mingw64\bin`
  must stay on `PATH` when running `kat`.

## Quick check

Run these in a scratch directory (they create a `.kat/` repository there):

```bash
kat init
kat create requirement --title "User authentication"
kat show <element-id>     # element_id printed by kat create
kat history
```

See `README.md` for an overview and `docs/cli.md` for the command contract.
