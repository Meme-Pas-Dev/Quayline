#!/bin/sh
# Métadonnées AppStream avec la version réelle : « <version>-<build> » (ex. 1.6.1-93),
# lue dans tauri.conf.json et build-info, et la date de construction.
# Argument : nom du fichier .desktop installé (Flatpak : dev.memepas.quayline.desktop,
# RPM : Quayline.desktop).
set -eu
desktop=${1:?usage: metainfo.sh <fichier.desktop>}
version=$(sed -n 's/^ *"version": *"\([^"]*\)".*/\1/p' src-tauri/tauri.conf.json | head -n1)
build=$(head -n1 build-info 2>/dev/null || echo dev)
sed -e "s/@VERSION@/$version-$build/" -e "s/@DATE@/$(date -u +%F)/" -e "s/@DESKTOP_ID@/$desktop/" \
    packaging/linux/metainfo.xml
