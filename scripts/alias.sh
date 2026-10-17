#!/usr/bin/env bash
# Manage the optional ck hard link beside the cinekey binary.
set -euo pipefail

operation=${1:?expected install or uninstall}
cinekey_bin=${2:?expected the installed cinekey binary}
alias_path="$(dirname "$cinekey_bin")/ck"

is_cinekey() {
    local version
    version=$("$1" --version 2>/dev/null) || return 1
    [[ $version =~ ^cinekey\ [0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]
}

if [[ -e $alias_path || -L $alias_path ]]; then
    if [[ ! $alias_path -ef $cinekey_bin ]] && ! is_cinekey "$alias_path"; then
        printf 'Keeping %s: it does not belong to cinekey.\n' "$alias_path" >&2
        exit 0
    fi
fi

case $operation in
    install)
        script_dir=$(cd "$(dirname "$0")" && pwd)
        expected_version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$script_dir/../Cargo.toml" | head -n 1)
        actual_version=$("$cinekey_bin" --version 2>/dev/null) || {
            printf 'Cannot create ck: %s is not an installed cinekey binary.\n' "$cinekey_bin" >&2
            exit 1
        }
        if [[ $actual_version != "cinekey $expected_version" ]]; then
            printf 'Cannot create ck: PATH selects %s (%s), expected cinekey %s. Check the Cargo install directory in PATH.\n' "$cinekey_bin" "$actual_version" "$expected_version" >&2
            exit 1
        fi
        if [[ $alias_path -ef $cinekey_bin ]]; then
            exit 0
        fi
        ln -f "$cinekey_bin" "$alias_path"
        ;;
    uninstall)
        if [[ -e $alias_path || -L $alias_path ]]; then
            rm "$alias_path"
        fi
        ;;
    *)
        printf 'Unknown alias operation: %s\n' "$operation" >&2
        exit 2
        ;;
esac
