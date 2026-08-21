#!/usr/bin/env bash
set -euo pipefail

# Default prefix to ~/.local for user-level installation without sudo
PREFIX="${PREFIX:-$HOME/.local}"
UNINSTALL=false
REPO_URL="${KAT_REPO_URL:-https://github.com/josucueva/kat.git}"
BRANCH="${KAT_BRANCH:-main}"

# Check if running inside a KAT source tree
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || echo "")"
IS_IN_REPO=false

if [[ -n "${SCRIPT_DIR}" && -f "${SCRIPT_DIR}/Cargo.toml" ]] && grep -q 'name = "kat"' "${SCRIPT_DIR}/Cargo.toml" 2>/dev/null; then
    IS_IN_REPO=true
    cd "${SCRIPT_DIR}"
elif [[ -f "Cargo.toml" ]] && grep -q 'name = "kat"' "Cargo.toml" 2>/dev/null; then
    IS_IN_REPO=true
fi

# Parse arguments
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
        --uninstall)
            UNINSTALL=true
            shift 1
            ;;
        -h|--help)
            echo "Usage: ./install.sh [--prefix PATH] [--uninstall]"
            echo "Installs or uninstalls KAT binary, UNIX man pages, and shell completions."
            echo "Default prefix: $HOME/.local"
            echo ""
            echo "One-command install:"
            echo "  curl -fsSL https://raw.githubusercontent.com/josucueva/kat/main/install.sh | bash"
            exit 0
            ;;
        *)
            echo "Error: Unknown option $1" >&2
            echo "Usage: ./install.sh [--prefix PATH] [--uninstall]" >&2
            exit 1
            ;;
    esac
done

BIN_DIR="${PREFIX}/bin"
MAN_DIR="${PREFIX}/share/man/man1"
BASH_COMP_DIR="${PREFIX}/share/bash-completion/completions"
ZSH_COMP_DIR="${PREFIX}/share/zsh/site-functions"
FISH_COMP_DIR="${PREFIX}/share/fish/vendor_completions.d"

if [[ "${UNINSTALL}" == "true" ]]; then
    echo "Uninstalling KAT from ${PREFIX}..."

    # Remove binary
    if [[ -f "${BIN_DIR}/kat" ]]; then
        echo "==> Removing binary ${BIN_DIR}/kat..."
        rm -f "${BIN_DIR}/kat"
    fi

    # Remove man pages
    if [[ -d "${MAN_DIR}" ]]; then
        echo "==> Removing man pages from ${MAN_DIR}..."
        rm -f "${MAN_DIR}/kat.1" "${MAN_DIR}"/kat-*.1
    fi

    # Remove shell completions
    echo "==> Removing shell completions..."
    rm -f "${BASH_COMP_DIR}/kat"
    rm -f "${ZSH_COMP_DIR}/_kat"
    rm -f "${FISH_COMP_DIR}/kat.fish"

    # Refresh man database if mandb exists
    if command -v mandb >/dev/null 2>&1; then
        echo "==> Updating man database..."
        mandb -q "${PREFIX}/share/man" 2>/dev/null || true
    fi

    echo ""
    echo "Uninstallation complete!"
    exit 0
fi

# Helper to detect current OS and architecture target
detect_target() {
    local os arch target_os target_arch
    os="$(uname -s | tr '[:upper:]' '[:lower:]')"
    case "${os}" in
        linux*)  target_os="unknown-linux-gnu" ;;
        darwin*) target_os="apple-darwin" ;;
        *)       return 1 ;;
    esac

    arch="$(uname -m)"
    case "${arch}" in
        x86_64|amd64)   target_arch="x86_64" ;;
        aarch64|arm64)  target_arch="aarch64" ;;
        *)              return 1 ;;
    esac

    echo "${target_arch}-${target_os}"
}

# If executed remotely (e.g. via `curl | bash`), try prebuilt release first, fallback to source build
if [[ "${IS_IN_REPO}" != "true" ]]; then
    TMP_DIR=$(mktemp -d 2>/dev/null || mktemp -d -t 'kat-install')
    cleanup() {
        rm -rf "${TMP_DIR}"
    }
    trap cleanup EXIT INT TERM

    TARGET=$(detect_target 2>/dev/null || echo "")

    # 1. Attempt downloading prebuilt release archive from GitHub Releases
    if [[ -n "${TARGET}" ]]; then
        ARCHIVE_NAME="kat-${TARGET}.tar.gz"
        RELEASE_URL="https://github.com/josucueva/kat/releases/latest/download/${ARCHIVE_NAME}"
        echo "==> Checking for prebuilt release (${TARGET})..."
        if curl -fsSL "${RELEASE_URL}" -o "${TMP_DIR}/${ARCHIVE_NAME}" 2>/dev/null; then
            echo "==> Extracting release archive..."
            tar -xzf "${TMP_DIR}/${ARCHIVE_NAME}" -C "${TMP_DIR}"
            EXTRACTED_DIR="${TMP_DIR}/kat-${TARGET}"

            mkdir -p "${BIN_DIR}" "${MAN_DIR}" "${BASH_COMP_DIR}" "${ZSH_COMP_DIR}" "${FISH_COMP_DIR}"

            echo "==> Installing binary to ${BIN_DIR}/kat..."
            install -m 755 "${EXTRACTED_DIR}/kat" "${BIN_DIR}/kat"

            echo "==> Installing man pages to ${MAN_DIR}..."
            for manpage in "${EXTRACTED_DIR}"/generated/man/*.1; do
                if [[ -f "${manpage}" ]]; then
                    install -m 644 "${manpage}" "${MAN_DIR}/"
                fi
            done

            echo "==> Installing shell completions..."
            if [[ -f "${EXTRACTED_DIR}/generated/completions/kat.bash" ]]; then
                install -m 644 "${EXTRACTED_DIR}/generated/completions/kat.bash" "${BASH_COMP_DIR}/kat"
            fi
            if [[ -f "${EXTRACTED_DIR}/generated/completions/_kat" ]]; then
                install -m 644 "${EXTRACTED_DIR}/generated/completions/_kat" "${ZSH_COMP_DIR}/_kat"
            fi
            if [[ -f "${EXTRACTED_DIR}/generated/completions/kat.fish" ]]; then
                install -m 644 "${EXTRACTED_DIR}/generated/completions/kat.fish" "${FISH_COMP_DIR}/kat.fish"
            fi

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
            exit 0
        fi
    fi

    # 2. Fallback: clone and build from source
    echo "==> Prebuilt binary not available, falling back to building from source..."
    if ! command -v git >/dev/null 2>&1; then
        echo "Error: 'git' is required to fetch KAT repository." >&2
        exit 1
    fi
    if ! command -v cargo >/dev/null 2>&1; then
        echo "Error: 'cargo' (Rust toolchain) is required to build KAT from source." >&2
        echo "Please install Rust first via https://rustup.rs" >&2
        exit 1
    fi

    echo "==> Fetching KAT from ${REPO_URL} (${BRANCH})..."
    git clone --depth 1 --branch "${BRANCH}" "${REPO_URL}" "${TMP_DIR}/kat-src" >/dev/null 2>&1 || \
    git clone --depth 1 "${REPO_URL}" "${TMP_DIR}/kat-src"

    echo "==> Running KAT installer..."
    "${TMP_DIR}/kat-src/install.sh" "$@"
    exit $?
fi

echo "Installing KAT to ${PREFIX}..."

# 1. Build release binary
echo "==> Building KAT release binary..."
cargo build --release

# 2. Ensure release assets are generated
echo "==> Generating UNIX man pages and shell completions..."
cargo run --bin generate_assets

# 3. Create destination directories
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
