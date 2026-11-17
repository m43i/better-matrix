#!/bin/sh
# All paths are positional arguments, never interpolated into shell source.
set -eu
target=$1
staged=$2
parent=$3
work=$4
error_file=$5
backup="$work/previous.app"

fail() {
    trap - EXIT
    printf '%s\n' 'failed' > "$error_file" || true
    if [ -e "$backup" ]; then
        rm -rf "$target"
        # Keep the backup if restoring fails; never remove the last usable version.
        mv "$backup" "$target" || exit 1
    fi
    /usr/bin/open -n -a "$target" || true
    rm -rf "$work"
    exit 1
}

[ -d "$target" ]
[ -d "$staged" ]
: > "$work/ready"
tries=0
while kill -0 "$parent" 2>/dev/null; do
    tries=$((tries + 1))
    # The app stayed open: leave its install alone.
    if [ "$tries" -gt 120 ]; then
        printf '%s\n' 'timeout' > "$error_file" || true
        rm -rf "$work"
        exit 1
    fi
    sleep 1
done
trap fail EXIT
mv "$target" "$backup"
mv "$staged" "$target"
/usr/bin/open -n -a "$target"
trap - EXIT
rm -rf "$work"
