#!/bin/sh
# Texte de la release d'une version : sa section de CHANGELOG.md (« ## [X.Y.Z] … »),
# sans le titre. Échoue si la section est absente ou vide.
set -eu
version=${1:?usage: release-notes.sh <version>}
notes=$(awk -v v="$version" '
    /^## \[/ { inside = index($0, "## [" v "]") == 1; next }
    inside
' "${2:-CHANGELOG.md}" | sed -e '/./,$!d')
[ -n "$(printf '%s' "$notes" | tr -d '[:space:]')" ] || { echo "CHANGELOG.md : aucune section pour $version." >&2; exit 1; }
printf '%s\n' "$notes"
