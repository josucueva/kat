#!/usr/bin/env bash
set -euo pipefail

# Default prefix to ~/.local for user-level installation without sudo
PREFIX="${PREFIX:-$HOME/.local}"

# Parse optional --prefix argument
while [[ $# -gt 0 ]]; do
    case "$1" in
        --prefix)
            PREFIX="$2"
            shift 2
            ;;
        --prefix=*)
            PREFIX="${1#*=}"
            shift 1
            ;;
        -h|--help)
            echo "Usage: ./install.sh [--prefix PATH]"
            echo "Installs KAT binary, UNIX man pages, and shell completions."
            echo "Default prefix: $HOME/.local"
            exit 0
            ;;
        *)
            echo "Error: Unknown option $1" >&2
            echo "Usage: ./install.sh [--prefix PATH]" >&2
            exit 1
            ;;
    esac
done

echo "Installing KAT to ${PREFIX}..."

# 1. Build release binary
echo "==> Building KAT release binary..."
cargo build --release

# 2. Ensure release assets are generated
echo "==> Generating UNIX man pages and shell completions..."
cargo run --bin generate_assets

# 3. Create destination directories
BIN_DIR="${PREFIX}/bin"
MAN_DIR="${PREFIX}/share/man/man1"
BASH_COMP_DIR="${PREFIX}/share/bash-completion/completions"
ZSH_COMP_DIR="${PREFIX}/share/zsh/site-functions"
FISH_COMP_DIR="${PREFIX}/share/fish/vendor_completions.d"

mkdir -p "${BIN_DIR}" "${MAN_DIR}" "${BASH_COMP_DIR}" "${ZSH_COMP_DIR}" "${FISH_COMP_DIR}"

# 4. Install binary
echo "==> Installing binary to ${BIN_DIR}/kat..."
install -m 755 target/release/kat "${BIN_DIR}/kat"

# 5. Install man pages
echo "==> Installing man pages to ${MAN_DIR}..."
for manpage in generated/man/*.1; do
    install -m 644 "${manpage}" "${MAN_DIR}/"
done

# 6. Install shell completions
echo "==> Installing shell completions..."
install -m 644 generated/completions/kat.bash "${BASH_COMP_DIR}/kat"
install -m 644 generated/completions/_kat "${ZSH_COMP_DIR}/_kat"
install -m 644 generated/completions/kat.fish "${FISH_COMP_DIR}/kat.fish"

# 7. Refresh man database if mandb exists
if command -v mandb >/dev/null 2>&1; then
    echo "==> Updating man database..."
    mandb -q "${PREFIX}/share/man" 2>/dev/null || true
fi

echo ""
echo "Installation complete!"
echo "Binary:      ${BIN_DIR}/kat"
echo "Man Pages:   ${MAN_DIR}/"
echo "Completions: Bash, Zsh, Fish"
echo ""
echo "Ensure ${BIN_DIR} is on your PATH."
