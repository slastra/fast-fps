#!/bin/bash
# Install what this project needs on Arch Linux and its relatives (CachyOS,
# EndeavourOS, Manjaro).
#
#   scripts/install-deps.sh           install anything that is missing
#   scripts/install-deps.sh --check   only report; change nothing
#
# It installs system packages with pacman (asking for your password through
# sudo) and never touches your Rust setup or this project's files. Running it
# again is harmless: it skips what is already there.
set -u

CHECK=0
case "${1:-}" in
--check) CHECK=1 ;;
"") ;;
*) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac

if ! command -v pacman >/dev/null 2>&1; then
    echo "This script is for Arch-based systems (it needs pacman)." >&2
    echo "On other systems, install the equivalents of the packages listed in AGENTS.md." >&2
    exit 2
fi

# What each package is for, so the list can be trimmed with open eyes.
BUILD=(
    base-devel        # compiler, linker, pkgconf
    alsa-lib          # sound
    systemd-libs      # libudev, for input devices
    wayland           # windows on Wayland
    libxkbcommon      # keyboard layouts
    vulkan-icd-loader # finds the GPU's Vulkan driver
)
TEST=(
    sway              # invisible displays for scripts/harness.sh
)
OPTIONAL=(
    imagemagick       # lets the harness warn about blank screenshots
)

# The Vulkan driver depends on the graphics card.
gpu=$(lspci 2>/dev/null | grep -i -E 'vga|3d|display' || true)
DRIVER=()
case "$gpu" in *Intel*) DRIVER+=(vulkan-intel) ;; esac
case "$gpu" in *AMD* | *ATI* | *Radeon*) DRIVER+=(vulkan-radeon) ;; esac
nvidia=0
case "$gpu" in *NVIDIA* | *nVidia*) nvidia=1 ;; esac

# Strip the trailing "# why" comments from the lists above.
names() { printf '%s\n' "$@" | grep -v '^#' | grep -E '^[a-z0-9][a-z0-9.+-]*$'; }

missing=()
report() {
    local label=$1
    shift
    echo "$label"
    local pkg
    for pkg in $(names "$@"); do
        if pacman -Q "$pkg" >/dev/null 2>&1; then
            echo "  ok       $pkg"
        else
            echo "  MISSING  $pkg"
            missing+=("$pkg")
        fi
    done
}

report "To build and run the game:" "${BUILD[@]}" "${DRIVER[@]:-}"
report "To run the test harness:" "${TEST[@]}"
report "Nice to have:" "${OPTIONAL[@]}"

problems=0
echo "Graphics:"
if [ -z "$gpu" ]; then
    echo "  ??       couldn't detect a graphics card (is pciutils installed?)"
elif [ $nvidia = 1 ] && ! pacman -Q nvidia-utils >/dev/null 2>&1; then
    echo "  MISSING  an NVIDIA driver. Install the one for your card (see the Arch wiki, NVIDIA);"
    echo "           this script won't pick one for you."
    problems=1
elif ls /usr/share/vulkan/icd.d/*.json >/dev/null 2>&1; then
    echo "  ok       Vulkan driver present"
else
    echo "  MISSING  no Vulkan driver found yet (the packages above should provide one)"
fi

echo "Rust:"
if command -v cargo >/dev/null 2>&1; then
    echo "  ok       $(cargo --version)"
else
    echo "  MISSING  cargo. Install Rust with:  sudo pacman -S rustup && rustup default stable"
    problems=1
fi

echo
if [ ${#missing[@]} -eq 0 ]; then
    echo "Nothing to install."
elif [ $CHECK = 1 ]; then
    echo "Missing: ${missing[*]}"
    echo "Run scripts/install-deps.sh (without --check) to install them."
    problems=1
else
    echo "Installing: ${missing[*]}"
    if sudo pacman -S --needed "${missing[@]}"; then
        echo "Installed."
    else
        echo "pacman did not finish; nothing else was changed." >&2
        exit 1
    fi
fi

cat <<'NOTE'

About the first build: `cargo run` compiles the whole game engine the first
time. On a laptop that can take an hour or more, and it will look stuck. It
isn't. Run one build at a time (two at once just fight over memory). After
that first build, changes to the game rebuild far more quickly.
NOTE
exit $problems
