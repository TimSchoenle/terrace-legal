#!/usr/bin/env bash
#
# Emits this repository's half of the README render payload as strict JSON on stdout.
#
# It is the `extra` input of the `readme-variables` action in docs.yml, deep-merged over what that
# action derives. The keys below are top-level and so collide with nothing it emits, which sits
# under `repo`, `release`, `toolchain` and `docs`.
#
# `Cargo.toml` is the single source of truth for both numbers the README quotes: the tag its
# install snippet pins, and the MSRV its badge advertises. The action reads both from the same
# manifest, as `release.tag` and `toolchain.msrv`, and the template reads them from there — so
# this script currently duplicates two facts rather than adding any. It stays because it is the
# hook a real generator plugs into, and because deleting it would take the `extra` wiring with it.
#
# Run it yourself to see what CI will render with:
#
#     bash .github/scripts/readme-variables.sh
#
# With `--emit-manifest <path>` it instead writes a one-package manifest to <path> and prints
# nothing. The `readme-variables` action reads a `[package]` table with literal values, and this
# repository has neither: the root is a virtual workspace and its members inherit every field with
# `version.workspace = true`. The emitted file carries the `[workspace.package]` values in the
# shape the action reads, so `Cargo.toml` stays the only place they are written down. The action picks its parser by the
# file's basename, so <path> has to end in `Cargo.toml`.
#
# Deliberately POSIX tools only, no `jq`: it is not present in a default Git for Windows shell,
# and a script that only runs on the CI runner is a script nobody checks their edit against.
set -euo pipefail

emit_path=""
if [ "${1:-}" = "--emit-manifest" ]; then
    emit_path="${2:?readme-variables: --emit-manifest needs an output path}"
    shift 2
fi

manifest="${1:-Cargo.toml}"

# Reads a top-level `key = "value"` from the manifest and rejects anything that would need JSON
# escaping. Both fields are version strings, so the accepted alphabet is the whole contract —
# and constraining it is what makes the `printf` at the bottom safe without a JSON encoder.
#
# Only `[package]` keys can match: a dependency's version sits inside an inline table
# (`figment = { version = "0.10", … }`) and never starts a line, so anchoring is enough.
field() {
    local key="$1" pattern="$2" value
    value="$(sed -n "s/^${key} = \"\([^\"]*\)\".*/\1/p" "${manifest}" | head -n1)"

    if [ -z "${value}" ]; then
        echo "readme-variables: no top-level '${key}' in ${manifest}" >&2
        return 1
    fi

    if ! printf '%s' "${value}" | grep -Eq "${pattern}"; then
        echo "readme-variables: '${key} = \"${value}\"' is not a version string" >&2
        return 1
    fi

    printf '%s' "${value}"
}

# Free text, so it cannot use the version alphabet; it is only refused when it would need escaping.
description="$(field description '^[^"\]+$')"
license="$(field license '^[0-9A-Za-z][0-9A-Za-z.+ -]*$')"
version="$(field version '^[0-9A-Za-z][0-9A-Za-z.+-]*$')"
msrv="$(field rust-version '^[0-9]+(\.[0-9]+){0,2}$')"

if [ -n "${emit_path}" ]; then
    mkdir -p "$(dirname "${emit_path}")"
    {
        echo '[package]'
        echo 'name = "terrace-legal"'
        echo "description = \"${description}\""
        echo "version = \"${version}\""
        echo "rust-version = \"${msrv}\""
        echo "license = \"${license}\""
    } > "${emit_path}"
    exit 0
fi

printf '{"version":"%s","tag":"v%s","msrv":"%s"}\n' "${version}" "${version}" "${msrv}"
