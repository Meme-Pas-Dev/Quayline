#!/bin/sh
# Métadonnées AppStream avec la version réelle : « <version>-<build> » (ex. 0.1.0-12),
# lue dans tauri.conf.json et build-info, et la date de construction.
set -eu
version=$(node -p "require('./src-tauri/tauri.conf.json').version")
build=$(head -n1 build-info 2>/dev/null || echo dev)
sed -e "s/@VERSION@/$version-$build/" -e "s/@DATE@/$(date +%F)/" flatpak/metainfo.xml
