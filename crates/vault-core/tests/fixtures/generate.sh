#!/usr/bin/env bash
#
# Regenerates the golden fixtures from the XML in sources/.
#
# The fixtures are produced by keepassxc-cli, not by Coffer, so that the
# round-trip suite tests us against an external implementation rather than
# against our own writer. Run this only when a source changes, and commit the
# resulting .kdbx files.
#
# keepassxc-cli picks the KDBX version from the features a database uses, and
# offers no way to ask for one. That is how the three format variants below are
# produced: strip the elements that force a version up, and the writer drops to
# the next one down.
#
#   KDBX 4.1  QualityCheck, PreviousParentGroup, group Tags, custom icon Name,
#             custom icon LastModificationTime
#   KDBX 4.0  CustomData on a group or an entry
#   KDBX 3.1  everything else; attachments move into Meta/Binaries

set -euo pipefail

cd "$(dirname "$0")"

PASSWORD=coffer-test
KEYFILE_PASSWORD=coffer-keyfile

FORCES_41=(
    -e '/<QualityCheck>/d'
    -e '/<PreviousParentGroup>/d'
    -e 's|<Tags>group-tag</Tags>||'
    -e 's|<Name>one pixel</Name>||'
    -e 's|<LastModificationTime>2026-01-01T00:00:00Z</LastModificationTime>||g'
)

FORCES_40=( -e '/<CustomData>/d' )

# keepass 0.13.22 cannot deserialise an empty Meta/Binaries entry, which is
# where KDBX 3 keeps attachments, so the 3.1 variant carries no zero-byte
# attachment. legacy-empty-attachment.kdbx pins that limitation on its own.
NO_EMPTY_ATTACHMENT=(
    -e '/<Binary ID="1"/d'
    -e '/zero-byte.txt/d'
    -e '/escape.txt/d'
)

find_cli() {
    if command -v keepassxc-cli >/dev/null 2>&1; then
        command -v keepassxc-cli
        return
    fi
    local mac=/Applications/KeePassXC.app/Contents/MacOS/keepassxc-cli
    if [ -x "$mac" ]; then
        echo "$mac"
        return
    fi
    echo "keepassxc-cli not found: install keepassxc" >&2
    exit 1
}

KP="$(find_cli)"
echo "using $KP ($("$KP" --version))"

version_of() {
    # Bytes 8..11 of a KDBX file are the minor and major version, little endian.
    od -An -tu2 -j8 -N4 "$1" | head -1 | awk '{printf "%d.%d", $2, $1}'
}

import() {
    local source=$1 target=$2 expected=$3 password=$4
    shift 4
    rm -f "$target"
    printf '%s\n%s\n' "$password" "$password" |
        "$KP" import -p "$@" "$source" "$target" >/dev/null
    chmod 0600 "$target"

    local actual
    actual="$(version_of "$target")"
    echo "  $target  KDBX $actual"
    if [ "$actual" != "$expected" ]; then
        echo "expected KDBX $expected, got KDBX $actual" >&2
        exit 1
    fi
}

echo "sources -> fixtures"

import sources/rich.xml rich-kdbx41.kdbx 4.1 "$PASSWORD"

sed "${FORCES_41[@]}" sources/rich.xml > sources/.tmp.xml
import sources/.tmp.xml rich-kdbx40.kdbx 4.0 "$PASSWORD"

sed "${FORCES_41[@]}" "${FORCES_40[@]}" "${NO_EMPTY_ATTACHMENT[@]}" \
    sources/rich.xml > sources/.tmp.xml
import sources/.tmp.xml rich-kdbx31.kdbx 3.1 "$PASSWORD"
rm -f sources/.tmp.xml

import sources/legacy-empty-attachment.xml legacy-empty-attachment-kdbx31.kdbx 3.1 "$PASSWORD"

import sources/minimal.xml minimal-kdbx41.kdbx 4.1 "$PASSWORD"
import sources/whitespace.xml whitespace-kdbx41.kdbx 4.1 "$PASSWORD"

# A database that needs a key file as well as a password. Coffer reads these,
# and never creates one.
( umask 077 && head -c 128 /dev/urandom > keyfile.key )
import sources/minimal.xml keyfile-kdbx41.kdbx 4.1 "$KEYFILE_PASSWORD" --set-key-file keyfile.key

# An empty database straight out of keepassxc-cli, with nothing but a root
# group. It is the only fixture the CLI creates on its own, so it is also the
# only one carrying the CLI's default KDF.
rm -f empty-kdbx31.kdbx
printf '%s\n%s\n' "$PASSWORD" "$PASSWORD" |
    "$KP" db-create -p empty-kdbx31.kdbx >/dev/null
chmod 0600 empty-kdbx31.kdbx
echo "  empty-kdbx31.kdbx  KDBX $(version_of empty-kdbx31.kdbx)"

echo "done"
