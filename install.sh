#!/usr/bin/env bash
#
# Installs Coffer into /Applications.
#
# The builds are not signed with a Developer ID and are not notarised, and that
# is the whole reason this script exists rather than a link to a disk image.
# curl does not put a quarantine attribute on what it fetches, so an application
# copied out of an image this script downloaded opens on the first try. A
# browser does put one on, and since macOS 15 there is no right click past it.
#
# Nothing is installed before its SHA-256 matches the checksums published beside
# the release. Nothing is written outside /Applications and one temporary
# directory that goes on the way out.
#
#   COFFER_VERSION  install this version rather than the newest release
#   COFFER_APPDIR   install somewhere other than /Applications

set -euo pipefail

REPOSITORY=timhartmann7/coffer
APPLICATIONS=${COFFER_APPDIR:-/Applications}

say() {
    printf '%s\n' "$1"
}

die() {
    printf '%s\n' "$1" >&2
    exit 1
}

# Says which of the things that go wrong went wrong. curl's own answer to a
# missing asset is a status line, which tells a reader nothing about what to do
# next. The status comes from the last response rather than from curl's exit
# code, because a release download is a redirect and an error after one is not
# the same exit code as an error before it.
fetch() {
    local url=$1 into=$2 status=0 code=000
    code=$(curl -sSL --proto '=https' --tlsv1.2 -o "$into" -w '%{http_code}' "$url") || status=$?

    if [ "$status" -ne 0 ]; then
        case $status in
            6 | 7 | 28 | 35) die "Could not reach github.com. Check the network and run this again." ;;
            *) die "curl gave up on $url with status $status." ;;
        esac
    fi

    case $code in
        200) ;;
        404) die "This release carries no ${url##*/}. See https://github.com/$REPOSITORY/releases and set COFFER_VERSION to one that does." ;;
        *) die "github.com answered $code for ${url##*/}." ;;
    esac
}

[ "$(uname -s)" = Darwin ] || die "Coffer is a macOS application, and this is $(uname -s)."
command -v curl >/dev/null || die "curl is not on the path, and this script has no other way to fetch anything."

# A shell under Rosetta reports x86_64 on a Mac that is not one, and the Intel
# build would run there, slowly, for nothing. proc_translated is what tells the
# two apart; a real Intel Mac does not have the key at all.
machine=$(uname -m)
if [ "$machine" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = 1 ]; then
    machine=arm64
fi

case $machine in
    arm64) architecture=aarch64 ;;
    x86_64) architecture=x64 ;;
    *) die "Coffer is built for arm64 and x86_64, and this Mac reports $machine." ;;
esac

# The tag comes off the redirect on /releases/latest rather than out of the API.
# The API allows sixty unauthenticated calls an hour from one address, which an
# office shares and runs out of, and a redirect needs no JSON parser.
version=${COFFER_VERSION:-}
if [ -z "$version" ]; then
    # Without -f, so that github.com answering can be told apart from github.com
    # not being reachable. With -f every one of them is curl exiting 22, and the
    # reader is told to check a network that is working.
    latest=$(curl -sSLI -o /dev/null -w '%{http_code} %{url_effective}' \
        "https://github.com/$REPOSITORY/releases/latest") ||
        die "Could not reach github.com to find the newest release."

    case ${latest%% *} in
        200) ;;
        404) die "github.com has no $REPOSITORY to take a release from." ;;
        *) die "github.com answered ${latest%% *} when asked for the newest release." ;;
    esac

    # A release redirects to its own tag. A repository that has none redirects to
    # the list instead, which ends in the word rather than in a version.
    version=${latest##*/}
    [ "$version" != releases ] ||
        die "There are no releases of Coffer yet. See https://github.com/$REPOSITORY/releases."
fi
version=${version#v}

case $version in
    '' | *[!0-9.]*) die "Expected a version like 0.1.0, and got \"$version\"." ;;
esac

asset="Coffer_${version}_${architecture}.dmg"
base="https://github.com/$REPOSITORY/releases/download/v$version"

work=$(mktemp -d)
mount=$work/image

leave() {
    hdiutil detach "$mount" -quiet >/dev/null 2>&1 ||
        hdiutil detach "$mount" -force -quiet >/dev/null 2>&1 || true
    rm -rf "$work"
}
# A handler that only tidied up would return, and bash would carry on into the
# next line with the image unmounted underneath it: the run would go on to
# accuse the release of holding no Coffer.app, or to delete the installed copy
# and have nothing left to put back. So a signal ends the run. `leave` is
# idempotent, and the EXIT trap running it again costs nothing.
trap leave EXIT
trap 'leave; exit 130' INT
trap 'leave; exit 143' HUP TERM

say "Fetching Coffer $version for $architecture."
fetch "$base/$asset" "$work/$asset"
fetch "$base/checksums.txt" "$work/checksums.txt"

expected=$(awk -v name="$asset" '$2 == name || $2 == "*" name { print $1 }' "$work/checksums.txt")
[ -n "$expected" ] || die "checksums.txt in v$version does not mention $asset."

actual=$(shasum -a 256 "$work/$asset" | awk '{ print $1 }')
if [ "$expected" != "$actual" ]; then
    rm -f "$work/$asset"
    die "$asset does not match its published checksum, and has been deleted.
  expected $expected
  received $actual
Do not install this file. Try again, and if it happens twice say so at
https://github.com/$REPOSITORY/issues."
fi

[ -d "$APPLICATIONS" ] || die "$APPLICATIONS is not there."
[ -w "$APPLICATIONS" ] || die "$APPLICATIONS is not writable by $(id -un). Ask an administrator, or install somewhere you can write with COFFER_APPDIR=\"\$HOME/Applications\"."

# A running Coffer may be holding a vault. Taking the application out from under
# it is not this script's decision to make.
if pgrep -x Coffer >/dev/null 2>&1; then
    die "Coffer is running, and it may be holding a vault. Quit it and run this again."
fi

hdiutil attach "$work/$asset" -mountpoint "$mount" -nobrowse -readonly -quiet ||
    die "$asset downloaded and matched its checksum, and macOS still would not mount it."

[ -d "$mount/Coffer.app" ] || die "There is no Coffer.app inside $asset."

# The old copy goes before the new one arrives rather than being written over.
# macOS will not let a terminal change an application bundle in place without App
# Management permission, and half a bundle is worse than none.
rm -rf "${APPLICATIONS:?}/Coffer.app"
ditto "$mount/Coffer.app" "$APPLICATIONS/Coffer.app"
hdiutil detach "$mount" -quiet

# curl sets no quarantine attribute, so on most Macs this removes nothing. It
# costs one call, and what it prevents is a first launch macOS refuses with no
# way through that a right click can reach any more.
xattr -dr com.apple.quarantine "$APPLICATIONS/Coffer.app"

say "Coffer $version is in $APPLICATIONS. Open it, and it will offer to make a vault."
