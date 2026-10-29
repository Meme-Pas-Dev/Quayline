#!/usr/bin/env bash
# Prépare les fichiers de la soumission Flathub (dépôt flathub/dev.memepas.quayline) :
# manifeste compilé sans réseau, et listes des sources Cargo et npm.
#
# Utilisation :
#     scripts/flathub.sh <dossier> vX.Y.Z    # sources : étiquette du dépôt public (soumission)
#     scripts/flathub.sh <dossier> --local   # sources : cette copie (essai, CI)
#
# Les listes de sources sont produites par flatpak-builder-tools
# (flatpak-cargo-generator.py, flatpak-node-generator), téléchargés au besoin dans
# ~/.cache/quayline-flathub. Le dossier doit être vide ou absent.

set -euo pipefail

usage() { sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
[[ $# -eq 2 ]] || usage
out=$1
ref=$2
root=$(cd "$(dirname "$0")/.." && pwd)
public=https://github.com/Meme-Pas-Dev/Quayline.git

if [[ -e $out && -n $(ls -A "$out" 2>/dev/null) ]]; then
    echo "✗ $out n'est pas vide" >&2
    exit 1
fi
mkdir -p "$out"
out=$(cd "$out" && pwd)

# Outils de génération (une fois, dans le cache de l'utilisateur).
tools=${XDG_CACHE_HOME:-$HOME/.cache}/quayline-flathub
if [[ ! -x $tools/venv/bin/flatpak-node-generator ]]; then
    echo "→ installation de flatpak-builder-tools dans $tools"
    rm -rf "$tools" && mkdir -p "$tools"
    git clone -q --depth 1 https://github.com/flatpak/flatpak-builder-tools.git "$tools/src"
    python3 -m venv "$tools/venv"
    "$tools/venv/bin/pip" -q install aiohttp toml tomlkit "$tools/src/node"
fi

# Fichiers de verrouillage : ceux de l'étiquette (soumission) ou de la copie (essai).
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
if [[ $ref == --local ]]; then
    cp "$root/Cargo.lock" "$root/package-lock.json" "$root/package.json" "$work/"
    source_block="      - type: dir
        path: $root
        skip:
          - target
          - node_modules
          - dist
          - .flatpak-builder
          - build-flatpak
          - build-flathub
          - build-dir
          - flatpak_app
          - repo"
else
    [[ $ref =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || usage
    commit=$(git ls-remote "$public" "refs/tags/$ref" | cut -f1)
    [[ -n $commit ]] || { echo "✗ étiquette $ref absente du dépôt public" >&2; exit 1; }
    for f in Cargo.lock package-lock.json package.json; do
        git -C "$root" show "$ref:$f" > "$work/$f"
    done
    source_block="      - type: git
        url: $public
        tag: $ref
        commit: $commit
        x-checker-data:
          type: git
          tag-pattern: ^v([\\d.]+)\$"
fi

echo "→ sources Cargo"
"$tools/venv/bin/python3" "$tools/src/cargo/flatpak-cargo-generator.py" "$work/Cargo.lock" -o "$out/cargo-sources.json"
echo "→ sources npm"
(cd "$work" && "$tools/venv/bin/flatpak-node-generator" npm package-lock.json -o "$out/node-sources.json" >/dev/null)

cat > "$out/dev.memepas.quayline.yml" <<EOF
# Généré par scripts/flathub.sh (dépôt de développement de Quayline) : ne pas modifier
# à la main, régénérer pour chaque version.
id: dev.memepas.quayline
runtime: org.gnome.Platform
runtime-version: "50"
sdk: org.gnome.Sdk
sdk-extensions:
  - org.freedesktop.Sdk.Extension.rust-stable
  - org.freedesktop.Sdk.Extension.node22
command: quayline

finish-args:
  - --socket=wayland
  # Presse-papiers (copier-coller du terminal, copie des mots de passe) par XWayland ;
  # la fenêtre reste en Wayland natif.
  - --socket=x11
  - --share=ipc
  - --device=dri
  # Connexions SSH et SFTP.
  - --share=network
  # Explorateur SFTP : fichiers locaux du dossier personnel (envoi, réception, corbeille).
  - --filesystem=home
  # Clés de sécurité FIDO2 (hidraw) : pas de permission plus fine dans Flatpak.
  - --device=all
  # Déverrouillage par un fournisseur d'identité : clé rangée dans le trousseau du
  # système (GNOME Keyring, KWallet).
  - --talk-name=org.freedesktop.secrets
  # Verrouillage avec la session et à la mise en veille.
  - --talk-name=org.freedesktop.ScreenSaver
  - --system-talk-name=org.freedesktop.login1

modules:
  - name: quayline
    buildsystem: simple
    build-options:
      append-path: /usr/lib/sdk/rust-stable/bin:/usr/lib/sdk/node22/bin
      env:
        CARGO_HOME: /run/build/quayline/cargo
        CARGO_NET_OFFLINE: "true"
        CARGO_PROFILE_RELEASE_DEBUG: "0"
        CARGO_INCREMENTAL: "0"
        npm_config_cache: /run/build/quayline/flatpak-node/npm-cache
        npm_config_offline: "true"
        # Mises à jour livrées par Flathub : pas de recherche de nouvelle version.
        QUAYLINE_UPDATES: store
    build-commands:
      - npm ci --offline --ignore-scripts --no-audit --no-fund
      - npx tauri build --no-bundle -- --locked --offline
      - install -Dm755 target/release/quayline /app/bin/quayline
      - >
        sed -e 's/{{name}}/Quayline/' -e 's/{{comment}}/Secure SSH\/SFTP client/'
        -e 's/{{exec}}/quayline/' -e 's/{{icon}}/dev.memepas.quayline/'
        -e 's/^StartupWMClass=.*/StartupWMClass=dev.memepas.quayline/'
        src-tauri/linux/app.desktop > dev.memepas.quayline.desktop
      - install -Dm644 dev.memepas.quayline.desktop /app/share/applications/dev.memepas.quayline.desktop
      - sh packaging/linux/metainfo.sh dev.memepas.quayline.desktop > metainfo.xml
      - install -Dm644 metainfo.xml /app/share/metainfo/dev.memepas.quayline.metainfo.xml
      - install -Dm644 src-tauri/icons/32x32.png /app/share/icons/hicolor/32x32/apps/dev.memepas.quayline.png
      - install -Dm644 src-tauri/icons/64x64.png /app/share/icons/hicolor/64x64/apps/dev.memepas.quayline.png
      - install -Dm644 src-tauri/icons/128x128.png /app/share/icons/hicolor/128x128/apps/dev.memepas.quayline.png
      - install -Dm644 src-tauri/icons/128x128@2x.png /app/share/icons/hicolor/256x256/apps/dev.memepas.quayline.png
      - install -Dm644 src-tauri/icons/app-icon.svg /app/share/icons/hicolor/scalable/apps/dev.memepas.quayline.svg
    sources:
$source_block
      - cargo-sources.json
      - node-sources.json
EOF
echo "✓ Fichiers Flathub prêts dans $out"
ls -1 "$out"
