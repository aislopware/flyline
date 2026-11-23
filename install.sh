#!/usr/bin/env bash
# Flyline installer
# Usage: source <(curl -sSfL https://github.com/aislopware/flyline/releases/latest/download/install.sh)
#   or:  curl -sSfL https://github.com/aislopware/flyline/releases/latest/download/install.sh | sh
#
# NOTE FOR MAINTAINERS:
# Both `source <(curl ...)` and traditional `curl ... | sh` (or `curl ... | bash`) MUST work
# and continue to be supported for posterity.
# - Sourcing via `source <(...)` automatically activates Flyline immediately in the current interactive session.
# - Running via `curl ... | sh` / `curl ... | bash` performs the installation and prints activation instructions.
# - Non-Bash environments (like `dash`) are safely caught by `verify_bash_environment()`.

if [ -n "${BASH_SOURCE:-}" ] && [ -n "${BASH_SOURCE[0]:-}" ] && [ "${BASH_SOURCE[0]}" != "$0" ]; then
    _FLYLINE_IS_SOURCED=true
    _FLYLINE_SAVED_OPTS="$(set +o)"
    _FLYLINE_SAVED_TRAP="$(trap -p EXIT || true)"
else
    _FLYLINE_IS_SOURCED=false
fi

set -eu

expand_path() {
    case "$1" in
        '~/'*) echo "${HOME}/${1#~/}" ;;
        '~')   echo "${HOME}" ;;
        *)     echo "$1" ;;
    esac
}

REPO="aislopware/flyline"
if [ -n "${FLYLINE_INSTALL_DIR:-}" ]; then
    INSTALL_DIR="$(expand_path "$FLYLINE_INSTALL_DIR")"
elif [ -n "${FLYLINE_LOAD_DIR:-}" ]; then
    INSTALL_DIR="$(expand_path "$FLYLINE_LOAD_DIR")"
else
    INSTALL_DIR="${HOME}/.local/lib"
fi
BASHRC="${HOME}/.bashrc"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

is_sourced() {
    [ "${_FLYLINE_IS_SOURCED:-false}" = "true" ]
}

is_interactive() {
    case "$-" in
        *i*) return 0 ;;
        *)   return 1 ;;
    esac
}

cleanup_flyline_install() {
    if [ -n "${TMP_DIR:-}" ] && [ -d "$TMP_DIR" ]; then
        rm -rf "$TMP_DIR"
    fi
    if [ "${_FLYLINE_IS_SOURCED:-false}" = "true" ]; then
        if [ -n "${_FLYLINE_SAVED_OPTS:-}" ]; then
            eval "$_FLYLINE_SAVED_OPTS" 2>/dev/null || true
        fi
        if [ -n "${_FLYLINE_SAVED_TRAP:-}" ]; then
            eval "$_FLYLINE_SAVED_TRAP" 2>/dev/null || true
        else
            trap - EXIT 2>/dev/null || true
        fi
        unset -f expand_path say warn err err_no_exit need_cmd download get_latest_version detect_os detect_arch detect_libc detect_bash_version_parts is_bash_version_4_4_or_later is_system_bash_pre_4_4 find_homebrew_bash verify_bash_environment verify_sha256 cleanup_flyline_install is_sourced is_interactive main 2>/dev/null || true
        unset REPO INSTALL_DIR BASHRC OS ARCH BREW_BASH TARGET LIB_NAME VERSION LIB_PATH ENABLE_CMD TMP_DIR _FLYLINE_IS_SOURCED _FLYLINE_SAVED_OPTS _FLYLINE_SAVED_TRAP 2>/dev/null || true
    fi
}

say() { printf '\033[1;34m==> \033[0m%s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
err_no_exit() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; }
err() {
    printf '\033[1;31merror:\033[0m %s\n' "$*" >&2
    cleanup_flyline_install
    if is_sourced; then
        return 1 2>/dev/null || exit 1
    else
        exit 1
    fi
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || err "Required command not found: $1"
}

download() {
    url="$1"
    dest="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -sSfL -o "$dest" "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$dest" "$url"
    else
        err "Neither curl nor wget is available. Please install one and retry."
    fi
}

get_latest_version() {
    url="https://github.com/${REPO}/releases/latest"
    if command -v curl >/dev/null 2>&1; then
        final_url="$(curl -sSfIL -o /dev/null -w '%{url_effective}' "$url" 2>/dev/null || true)"
    elif command -v wget >/dev/null 2>&1; then
        final_url="$(wget --max-redirect=0 --server-response -O /dev/null "$url" 2>&1 | grep -i 'location:' | head -1 || true)"
    else
        err "Neither curl nor wget is available. Please install one and retry."
    fi
    version="$(printf '%s' "$final_url" | sed 's|.*/||' | cut -d' ' -f1 | tr -d '\r\n')"
    [ -n "$version" ] || err "Could not determine latest version from GitHub Release redirect."
    echo "$version"
}

# ---------------------------------------------------------------------------
# Platform & Environment detection
# ---------------------------------------------------------------------------

detect_os() {
    if [ -n "${TERMUX_VERSION:-}" ] || [ -d "/data/data/com.termux" ] || (uname -o 2>/dev/null | grep -qi android) || (uname -a 2>/dev/null | grep -qi android); then
        echo "android"
        return
    fi
    os="$(uname -s)"
    case "$os" in
        Linux) echo "linux" ;;
        Darwin) echo "darwin" ;;
        *) err "Unsupported OS: $os. Prebuilt libraries exist for Linux, macOS and Android (Termux)." ;;
    esac
}

detect_arch() {
    arch="$(uname -m)"
    case "$arch" in
        x86_64 | amd64) echo "x86_64" ;;
        aarch64 | arm64) echo "aarch64" ;;
        *) err "Unsupported architecture: $arch. Prebuilt libraries exist for x86_64 and aarch64; build from source for others." ;;
    esac
}

detect_libc() {
    # 1. Inspect the interpreter of the running shell executable — most reliable.
    shell_exe="/proc/$$/exe"
    if [ ! -e "$shell_exe" ]; then
        shell_exe="$(command -v sh || true)"
    fi
    if [ -n "$shell_exe" ] && command -v readelf >/dev/null 2>&1; then
        interp="$(readelf -l "$shell_exe" 2>/dev/null | grep 'interpreter' | grep -o '\[.*\]' | tr -d '[]')" || true
        case "$interp" in
            *musl*) echo "musl"; return ;;
            *) echo "gnu"; return ;;
        esac
    fi

    # 2. Ask ldd directly — musl's ldd prints "musl libc" on --version.
    if ldd --version 2>&1 | grep -qi musl; then
        echo "musl"
        return
    fi

    # 3. Look for the musl dynamic linker on disk.
    if ls /lib/ld-musl-* >/dev/null 2>&1; then
        echo "musl"
        return
    fi

    # 4. Fall back to GNU libc.
    echo "gnu"
}

# Detect the version of the active/system bash as "major minor" integers.
detect_bash_version_parts() {
    if [ -n "${BASH_VERSINFO:-}" ]; then
        echo "${BASH_VERSINFO[0]} ${BASH_VERSINFO[1]}"
        return
    fi
    bash_bin="$(command -v bash 2>/dev/null || true)"
    [ -n "$bash_bin" ] || { echo "0 0"; return; }
    "$bash_bin" -c 'echo "${BASH_VERSINFO[0]} ${BASH_VERSINFO[1]}"' 2>/dev/null || echo "0 0"
}

# Returns 0 (true) if the given major.minor version is >= 4.4, 1 (false) otherwise.
is_bash_version_4_4_or_later() {
    major="$1"; minor="$2"
    [ "${major:-0}" -gt 4 ] || { [ "${major:-0}" -eq 4 ] && [ "${minor:-0}" -ge 4 ]; }
}

# Returns 0 (true) if the system bash is older than 4.4, 1 (false) otherwise.
is_system_bash_pre_4_4() {
    version_str="$(detect_bash_version_parts)"
    major="${version_str%% *}"
    minor="${version_str##* }"
    ! is_bash_version_4_4_or_later "$major" "$minor"
}

# Returns the path to a Homebrew-installed bash >= 4.4, or an empty string.
find_homebrew_bash() {
    for candidate in "/opt/homebrew/bin/bash" "/usr/local/bin/bash"; do
        if [ -x "$candidate" ]; then
            v="$("$candidate" -c 'echo "${BASH_VERSINFO[0]} ${BASH_VERSINFO[1]}"' 2>/dev/null || echo "0 0")"
            major="${v%% *}"; minor="${v##* }"
            if is_bash_version_4_4_or_later "$major" "$minor"; then
                echo "$candidate"
                return
            fi
        fi
    done
    echo ""
}

verify_bash_environment() {
    # 1. Ensure running inside Bash
    if [ -z "${BASH_VERSION:-}" ]; then
        err_no_exit "The flyline installer must be run from Bash."
        err "Please run the installer using Bash."
    fi

    # 2. Check required dependencies
    need_cmd tar
    if ! command -v curl >/dev/null 2>&1 && ! command -v wget >/dev/null 2>&1; then
        err "Neither curl nor wget is available. Please install one and retry."
    fi
}

# ---------------------------------------------------------------------------
# Helpers for portability
# ---------------------------------------------------------------------------

verify_sha256() {
    sha256_file="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum -c "$sha256_file"
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 -c "$sha256_file"
    else
        err "No checksum tool found (sha256sum or shasum). Cannot verify download."
    fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

main() {
    verify_bash_environment

    OS="$(detect_os)"
    ARCH="$(detect_arch)"

    # flyline needs bash >= 4.4, which calls <name>_builtin_load on `enable -f`.
    # Every supported Linux and Termux ships newer; macOS's /bin/bash is 3.2.
    if is_system_bash_pre_4_4; then
        BREW_BASH=""
        if [ "$OS" = "darwin" ]; then
            BREW_BASH="$(find_homebrew_bash)"
        fi
        if [ -n "$BREW_BASH" ]; then
            warn "Your system Bash is older than 4.4 and cannot load flyline."
            warn "Ensure that you use $BREW_BASH for flyline."
        elif [ "$OS" = "darwin" ]; then
            err_no_exit "Your system Bash is older than 4.4 and cannot load flyline."
            err_no_exit "To use flyline on macOS, please install a modern Bash using Homebrew:"
            err_no_exit "    brew install bash"
            err "Then re-run the installer using Homebrew Bash."
        else
            err "flyline needs Bash 4.4 or newer; this Bash is older."
        fi
    fi

    if [ "$OS" = "darwin" ]; then
        if [ "$ARCH" != "aarch64" ]; then
            err "Intel Macs have no prebuilt library; build from source with: cargo build --release --lib"
        fi
        TARGET="aarch64-apple-darwin"
        LIB_NAME="libflyline.dylib"
    elif [ "$OS" = "android" ]; then
        case "$ARCH" in
            aarch64)
                TARGET="aarch64-linux-android"
                ;;
            *)
                err "Unsupported Android architecture: $ARCH. Only aarch64 (arm64) is supported."
                ;;
        esac
        LIB_NAME="libflyline.so"
    else
        LIBC="$(detect_libc)"
        TARGET="${ARCH}-unknown-linux-${LIBC}"
        LIB_NAME="libflyline.so"
    fi

    say "Detected target: ${TARGET}"

    if [ -n "${FLYLINE_INSTALL_VERSION:-}" ]; then
        say "Using specified release version: ${FLYLINE_INSTALL_VERSION}"
        VERSION="${FLYLINE_INSTALL_VERSION}"
    else
        say "Fetching latest release information..."
        VERSION="$(get_latest_version)"
        say "Latest version: ${VERSION}"
    fi

    ARCHIVE_STEM="libflyline-${VERSION}-${TARGET}"

    ARCHIVE="${ARCHIVE_STEM}.tar.gz"
    ARCHIVE_SHA256="${ARCHIVE}.sha256"

    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE}"
    SHA256_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE_SHA256}"

    TMP_DIR="$(mktemp -d)"
    if ! is_sourced; then
        # shellcheck disable=SC2064
        trap "rm -rf '$TMP_DIR'" EXIT
    fi

    say "Downloading ${ARCHIVE} from
    ${DOWNLOAD_URL}..."
    download "$DOWNLOAD_URL" "${TMP_DIR}/${ARCHIVE}"

    if [ -n "$SHA256_URL" ]; then
        say "Downloading checksum from ${SHA256_URL}..."
        download "$SHA256_URL" "${TMP_DIR}/${ARCHIVE_SHA256}"

        say "Verifying checksum..."
        # Run from TMP_DIR so the relative path in the checksum file resolves.
        (cd "$TMP_DIR" && verify_sha256 "$ARCHIVE_SHA256") \
            || err "Checksum verification failed for ${ARCHIVE}."
    fi

    mkdir -p "$INSTALL_DIR"

    tar xzf "${TMP_DIR}/${ARCHIVE}" -C "$INSTALL_DIR"

    VERSION_NO_V="${VERSION#v}"
    LIB_VERSIONED="${LIB_NAME}.${VERSION_NO_V}"

    if [ -f "${INSTALL_DIR}/${LIB_VERSIONED}" ]; then
        say "Creating symlink ${LIB_NAME} -> ${LIB_VERSIONED}..."
        rm -f "${INSTALL_DIR}/${LIB_NAME}"
        (cd "$INSTALL_DIR" && ln -s "$LIB_VERSIONED" "$LIB_NAME")
    else
        if [ -f "${INSTALL_DIR}/${LIB_NAME}" ]; then
            warn "Expected to find versioned library ${LIB_VERSIONED}, but found ${LIB_NAME} instead."
        else
            err "Failed to find the installed library file in ${INSTALL_DIR}."
        fi
    fi

    LIB_PATH="${INSTALL_DIR}/${LIB_NAME}"
    say "Installed: ${LIB_PATH}"

    # Verify that the library can be loaded by system bash before updating ~/.bashrc
    if command -v bash >/dev/null 2>&1; then
        load_test="$(bash -c "enable -f '$LIB_PATH' flyline" 2>&1 || true)"
        if echo "$load_test" | grep -q "dlopen failed"; then
            warn "Failed to load ${LIB_PATH} with system bash (dlopen test failed)."
            warn "Skipping automatic modification of ${BASHRC}."
            warn "You can try loading it manually with:"
            warn "    enable flyline 2>/dev/null || enable -f \"${LIB_PATH}\" flyline"
            cleanup_flyline_install
            return 0
        fi
    fi

    # Update or add 'enable flyline 2>/dev/null || enable -f ... flyline' in ~/.bashrc.
    if [ -z "${FLYLINE_VERSION:-}" ]; then
        ENABLE_CMD="enable flyline 2>/dev/null || enable -f \"${LIB_PATH}\" flyline"
        printf '\n# Flyline - enhanced Bash experience\n%s\n' "$ENABLE_CMD" >> "$BASHRC"
        say "Added flyline to ${BASHRC}"
    else
        say "Flyline is already installed (detected ${FLYLINE_VERSION}); skipping .bashrc modification."
    fi

    # On macOS, login shells read ~/.bash_profile (not ~/.bashrc).
    # Warn the user if ~/.bash_profile does not appear to source ~/.bashrc.
    if [ "$OS" = "darwin" ]; then
        BASH_PROFILE="${HOME}/.bash_profile"
        if [ -f "$BASH_PROFILE" ]; then
            if ! grep -qE '(source|\.)[[:space:]]+(~|\$\{?HOME\}?)/\.bashrc([[:space:]]|$)' "$BASH_PROFILE"; then
                warn "Your ${BASH_PROFILE} does not appear to source ~/.bashrc."
                warn "On macOS, login shells read ~/.bash_profile, so flyline may not load in new terminals."
                warn "Consider adding the following to ${BASH_PROFILE}:"
                warn '    if [ -f ~/.bashrc ]; then . ~/.bashrc; fi'
            fi
        else
            warn "${BASH_PROFILE} does not exist."
            warn "On macOS, login shells read ~/.bash_profile, so flyline may not load in new terminals."
            warn "Consider creating ${BASH_PROFILE} with the following content:"
            warn '    if [ -f ~/.bashrc ]; then . ~/.bashrc; fi'
        fi
    fi

    say ""
    if [ -n "${FLYLINE_VERSION:-}" ]; then
        say "Upgrade from ${FLYLINE_VERSION} -> ${VERSION}, run \`flyline changelog\` to see what's changed."
        say "To activate the upgrade, open a new shell."
        if [ -n "${FLYLINE_LOAD_DIR:-}" ]; then
            resolved_load_dir="$(expand_path "$FLYLINE_LOAD_DIR")"
            if [ "$resolved_load_dir" != "$INSTALL_DIR" ]; then
                warn "The upgrade installation directory ($INSTALL_DIR) is different from the currently running load directory ($resolved_load_dir)."
                warn "Please make sure to update your ~/.bashrc or other startup scripts to point to the new libflyline."
            fi
        fi
    else
        say "Installation complete!"
        if is_sourced && is_interactive; then
            say "Activating flyline in your current shell session..."
            enable flyline 2>/dev/null || enable -f "${LIB_PATH}" flyline
            say "Flyline is now active!"
        else
            say '    To activate in the current shell:'
            say "        enable flyline 2>/dev/null || enable -f \"${LIB_PATH}\" flyline"
            say '    Or open a new terminal and run the tutorial:'
            say "        flyline run-tutorial"
        fi
    fi

    # Detect if ble.sh is running
    if [ -n "${BLE_SESSION_ID:-}" ] || [ -n "${_ble_version:-}" ]; then
        say ""
        warn "ble.sh (Bash Line Editor) is detected."
        warn "Please turn it off/disable it before starting flyline to avoid conflicts."
    fi

    cleanup_flyline_install
}

main "$@"
