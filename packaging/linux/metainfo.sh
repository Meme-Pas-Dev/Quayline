#!/bin/sh
# Métadonnées AppStream : gabarit metainfo.xml complété avec la version réelle, l'adresse
# des captures (dépôt public, à l'étiquette de la version) et les versions publiées du
# journal (CHANGELOG.md : date, et notes en anglais des cinq dernières).
# Argument : nom du fichier .desktop installé (Flatpak : dev.memepas.quayline.desktop,
# RPM et .deb : Quayline.desktop).
set -eu
desktop=${1:?usage: metainfo.sh <fichier.desktop>}
version=$(sed -n 's/^ *"version": *"\([^"]*\)".*/\1/p' src-tauri/tauri.conf.json | head -n1)

releases=$(awk -v current="$version" -v today="$(date -u +%F)" '
function esc(s) { gsub(/&/, "\\&amp;", s); gsub(/</, "\\&lt;", s); gsub(/>/, "\\&gt;", s); return s }
function plain(s) {
    gsub(/`/, "", s); gsub(/\*\*/, "", s)
    while (match(s, /\[[^]]*\]\([^)]*\)/)) {
        link = substr(s, RSTART, RLENGTH); sub(/\]\(.*/, "", link); sub(/^\[/, "", link)
        s = substr(s, 1, RSTART - 1) link substr(s, RSTART + RLENGTH)
    }
    return esc(s)
}
function flush() { if (item != "") { items = items "          <li>" plain(item) "</li>\n"; item = "" } }
function close_release() {
    flush()
    if (ver == "") return
    printf "    <release version=\"%s\" date=\"%s\"", ver, date
    if (n <= 5 && items != "") printf ">\n      <description>\n        <ul>\n%s        </ul>\n      </description>\n    </release>\n", items
    else printf "/>\n"
    items = ""
}
/^## \[/ {
    close_release(); en = 0
    v = $0; sub(/^## \[/, "", v); sub(/\].*/, "", v)
    if (v == "Unreleased") { ver = ""; next }
    ver = v; date = $0; sub(/.*\] - /, "", date); n++
    seen[ver] = 1
    next
}
ver == "" { next }
/^### English/ { en = 1; next }
/^### / { flush(); en = 0; next }
en && /^- / { flush(); item = substr($0, 3); next }
en && /^  [^ ]/ && item != "" { line = $0; sub(/^ +/, "", line); item = item " " line; next }
en && /^$/ { flush() }
END {
    close_release()
}
' CHANGELOG.md)

# Version en cours absente du journal (build de développement) : ajoutée en tête.
case "$releases" in
    *"version=\"$version\""*) ;;
    *) releases=$(printf '    <release version="%s" date="%s"/>\n%s' "$version" "$(date -u +%F)" "$releases") ;;
esac

screenshots="https://raw.githubusercontent.com/Meme-Pas-Dev/Quayline/v$version/packaging/flathub/screenshots"

awk -v releases="$releases" -v desktop="$desktop" -v shots="$screenshots" '
/@RELEASES@/ { print releases; next }
{ gsub(/@DESKTOP_ID@/, desktop); gsub(/@SCREENSHOTS@/, shots); print }
' packaging/linux/metainfo.xml
