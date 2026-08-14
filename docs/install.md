# Installation

The `kat` CLI is a single Rust binary that builds on **Linux** and **Windows**.

## Prerequisites

- [Rust](https://rustup.rs) (stable, via `rustup`)
- Git (to clone the repository)

## Standard Installation (Recommended)

From the repository root:

```bash
./install.sh
```

This automates:
1. Building `kat` in release mode (`cargo build --release`).
2. Generating UNIX man pages and shell completion scripts.
3. Installing the binary to `~/.local/bin/kat`.
4. Installing man pages to `~/.local/share/man/man1/`.
5. Installing shell completions for Bash, Zsh, and Fish.

Ensure `~/.local/bin` is on your `PATH`.

### System-Wide Installation

To install system-wide (requires write permissions to `/usr/local`):

```bash
sudo ./install.sh --prefix /usr/local
```

### Uninstallation

To remove the installed binary, man pages, and shell completions:

```bash
./install.sh --uninstall
# or
./uninstall.sh
```

For system-wide installations:

```bash
sudo ./install.sh --uninstall --prefix /usr/local
```

---

## Cargo Installation (Binary Only)

If you only want the executable installed via Cargo:

```bash
cargo install --path .
```

This installs `kat` into Cargo's bin directory:

- **Linux:** `~/.cargo/bin/kat`
- **Windows:** `%USERPROFILE%\.cargo\bin\kat.exe`

Note: `cargo install` only installs binary executables. To generate and install man pages and shell completions when using `cargo install`, run:

```bash
cargo run --bin generate_assets
```

---

## UNIX Man Pages and Shell Completions (Manual Setup)

If installing manually without `install.sh`:

Generate assets:

```bash
cargo run --bin generate_assets
```

This produces:

```text
generated/man/
generated/completions/
```

Generated assets are deterministic and can be verified with:

```bash
cargo run --bin generate_assets
git diff --exit-code generated/
```

### Man Pages

For a per-user installation on Linux:

```bash
mkdir -p ~/.local/share/man/man1
cp generated/man/*.1 ~/.local/share/man/man1/
```

Then refresh the man database if applicable:

```bash
mandb ~/.local/share/man
```

You can then view command documentation:

```bash
man kat
man kat-create
man kat-trace
```

### Bash

For the current session:

```bash
source generated/completions/kat.bash
```

For persistent installation, copy it into an appropriate Bash completion directory:

```bash
mkdir -p ~/.local/share/bash-completion/completions
cp generated/completions/kat.bash ~/.local/share/bash-completion/completions/kat
```

### Zsh

Add a directory containing `_kat` to `$fpath`, for example:

```bash
mkdir -p ~/.local/share/zsh/site-functions
cp generated/completions/_kat ~/.local/share/zsh/site-functions/
```

Ensure that directory is present in `fpath`, then run:

```bash
autoload -Uz compinit
compinit
```

### Fish

```bash
mkdir -p ~/.config/fish/completions
cp generated/completions/kat.fish ~/.config/fish/completions/
```

---

## Manual Build (No Install)

```bash
cargo build --release
./target/release/kat          # Linux
.\target\release\kat.exe      # Windows
```

---

## Platform Notes

### Linux

The native GNU toolchain is the default; no extra setup is required.

### Windows

- **With VS Build Tools (MSVC):** the default toolchain (`x86_64-pc-windows-msvc`) produces a self-contained `kat.exe`.
- **Without (MinGW/GNU):** set the machine-local override:

  ```powershell
  rustup override set stable-x86_64-pc-windows-gnu
  $env:PATH = "$env:USERPROFILE\.cargo\bin;C:\msys64\mingw64\bin;$env:PATH"
  cargo install --path .
  ```

---

## Quick Check

Run these in a scratch directory (they create a `.kat/` repository there):

```bash
kat init
kat create requirement --title "User authentication"
kat show <element-id>     # element_id printed by kat create
kat history
```

See `README.md` for an overview and `docs/cli.md` for the command contract.
