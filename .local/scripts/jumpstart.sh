#!/usr/bin/env bash
# Installation script for setting up the development environment. It finds the operating system and runs the matching
# script from jumpstart.d/, whose name is <os>-jumpstart.sh.

set -e

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

case "$(uname -s)" in
    Darwin)
        os=darwin
        ;;
    Linux)
        # Only Debian-based distributions, such as Ubuntu, are supported on Linux, as they use apt.
        os=""
        if [[ -r /etc/os-release ]]; then
            # shellcheck disable=SC1091
            . /etc/os-release
            case "${ID:-} ${ID_LIKE:-}" in
                *ubuntu*|*debian*) os=debian ;;
            esac
        fi
        ;;
    *)
        os=""
        ;;
esac

script="$here/jumpstart.d/${os}-jumpstart.sh"
if [[ -z "$os" || ! -f "$script" ]]; then
    echo "ERROR: No jumpstart script for this operating system ($(uname -s))" >&2
    exit 1
fi

exec bash "$script" "$@"
