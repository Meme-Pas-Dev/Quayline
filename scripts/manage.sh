#!/usr/bin/env bash
# Installe, met à jour ou supprime Quayline sur Fedora, en RPM ou en Flatpak.
#
# Le script demande d'abord le format (RPM ou Flatpak), puis l'action.
# Les releases se téléchargent sans compte GitHub ; les builds de développement
# exigent GitHub CLI connecté : sudo dnf install gh && gh auth login
#
# Utilisation :
#     scripts/manage.sh
#
# Pour le RPM, l'installation est confiée à install-latest.sh (même dossier), ou
# compilée sur place depuis le clone (sans GitHub Actions).

set -euo pipefail

APP_ID="dev.memepas.quayline"
APP_NAME="Quayline"
FLATPAK_WORKFLOW="flatpak.yml"
FLATPAK_ARTIFACT="quayline-flatpak"
BRANCH="main"
# Suit les liens (~/.local/bin → clone) pour retrouver la clé et les autres scripts.
SCRIPT_DIR=$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)
# shellcheck source=scripts/github.sh
source "$SCRIPT_DIR/github.sh"

tmp=""
trap 'rm -rf "${tmp:-}"' EXIT

# Vérifie la signature GPG d'un fichier d'empreintes (« <fichier>.asc ») avec la clé
# publique du dépôt (packaging/signing-key.asc). Tant que le dépôt n'a pas de clé, seule
# l'empreinte est vérifiée ; dès qu'il en a une, un paquet non signé est refusé.
verify_signature() {
    local sums=$1 key="$SCRIPT_DIR/../packaging/signing-key.asc"
    if [[ ! -f $key ]]; then
        info "Pas de clé de signature dans le dépôt : seule l'empreinte est vérifiée."
        return 0
    fi
    command -v gpgv >/dev/null || die "gpgv introuvable : sudo dnf install gnupg2"
    [[ -f $sums.asc ]] || die "Paquet non signé ($(basename "$sums").asc absent) : installation annulée."
    gpg --batch --quiet --dearmor <"$key" >"$sums.keyring" || die "Clé publique illisible : $key"
    gpgv --keyring "$sums.keyring" "$sums.asc" "$sums" >/dev/null 2>&1 \
        || die "Signature invalide : ce paquet n'a pas été signé par la clé du projet. Installation annulée."
    info "Signature vérifiée."
}

die() { echo "✗ $*" >&2; exit 1; }
info() { echo "→ $*"; }
ok() { echo "✓ $*"; }

# Pose une question fermée ; renvoie 0 pour oui. Réponse par défaut : non.
confirm() {
    local answer
    read -r -p "$1 [o/N] " answer
    [[ $answer =~ ^[oOyY]$ ]]
}

# Menu numéroté ; écrit le numéro choisi (à partir de 1) sur la sortie standard.
choose() {
    local prompt=$1; shift
    local i answer
    echo >&2
    echo "$prompt" >&2
    i=1
    for option in "$@"; do
        echo "  $i) $option" >&2
        i=$((i + 1))
    done
    while true; do
        read -r -p "Votre choix : " answer
        if [[ $answer =~ ^[0-9]+$ ]] && (( answer >= 1 && answer <= $# )); then
            echo "$answer"
            return
        fi
        echo "Choix invalide." >&2
    done
}

# ─────────────────────────── État actuel ───────────────────────────

rpm_name() {
    rpm -qa --queryformat '%{NAME}\n' 2>/dev/null | grep -ix "$APP_NAME" | head -n1 || true
}

rpm_version() {
    local name
    name=$(rpm_name)
    [[ -n $name ]] && rpm -q --queryformat '%{VERSION}-%{RELEASE}' "$name" 2>/dev/null
    return 0
}

flatpak_version() {
    command -v flatpak >/dev/null || return 0
    LC_ALL=C flatpak list --user --app --columns=application,version 2>/dev/null \
        | awk -v id="$APP_ID" '$1 == id { print $2; exit }' || true
    return 0
}

flatpak_installed() {
    command -v flatpak >/dev/null && flatpak info --user "$APP_ID" >/dev/null 2>&1
}

show_state() {
    local r f
    r=$(rpm_version)
    f=$(flatpak_version)
    echo "$APP_NAME sur cette machine :"
    echo "  RPM     : ${r:-non installé}"
    if flatpak_installed; then
        echo "  Flatpak : ${f:-installé (version inconnue)}"
    else
        echo "  Flatpak : non installé"
    fi
}

# ─────────────────────────────── RPM ───────────────────────────────

rpm_install() {
    local installer="$SCRIPT_DIR/install-latest.sh"
    [[ -f $installer ]] || die "install-latest.sh introuvable à côté de ce script."
    local mode
    mode=$(choose "Quelle version du RPM ?" \
        "La plus récente (release publiée, sinon dernier build de $BRANCH)" \
        "Dernier build de $BRANCH (développement)" \
        "Dernière release publiée uniquement" \
        "Compiler ici, depuis ce clone (sans GitHub Actions)")
    case "$mode" in
        1) bash "$installer" ;;
        2) bash "$installer" --dev ;;
        3) bash "$installer" --release ;;
        4) rpm_build_local ;;
    esac
}

# Compilation locale du RPM, sans attendre une release ni passer par GitHub Actions.
# Mêmes dépendances et même numéro de build que la CI (rpm.yml) ; le paquet
# n'est pas signé avec la clé du projet, mais il est construit à partir de ce clone.
BUILD_DEPS=(git gcc gcc-c++ make pkgconf-pkg-config file webkit2gtk4.1-devel openssl-devel
    librsvg2-devel systemd-devel rust cargo nodejs npm)

rpm_build_local() {
    local root build missing=() rpms
    root=$(cd "$SCRIPT_DIR/.." && pwd)
    git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1 \
        || die "$root n'est pas un clone git de $APP_NAME."
    if [[ -n $(git -C "$root" status --porcelain --untracked-files=no) ]]; then
        info "Attention : le clone contient des modifications locales, elles seront compilées."
    fi
    if confirm "Mettre d'abord le clone à jour (git pull) ?"; then
        git -C "$root" pull --ff-only || die "git pull a échoué."
    fi
    # Numéro de build : celui inscrit par la publication (clone du dépôt public), sinon
    # le nombre de commits (dépôt de développement), comme dans la CI.
    if [[ -s $root/build-info ]]; then
        build=$(head -n1 "$root/build-info")
    else
        [[ $(git -C "$root" rev-parse --is-shallow-repository) == false ]] \
            || die "Clone partiel : le numéro de build serait faux. Lancez : git -C $root fetch --unshallow"
        build=$(git -C "$root" rev-list --count HEAD)
    fi

    for pkg in "${BUILD_DEPS[@]}"; do
        rpm -q "$pkg" >/dev/null 2>&1 || missing+=("$pkg")
    done
    if (( ${#missing[@]} )); then
        info "Dépendances de compilation manquantes : ${missing[*]}"
        sudo dnf install -y --setopt=install_weak_deps=False "${missing[@]}"
    fi

    info "Compilation de $APP_NAME (build $build, commit $(git -C "$root" rev-parse --short HEAD)) :"
    info "une dizaine de minutes la première fois, moins ensuite."
    (
        cd "$root"
        npm ci --no-audit --no-fund
        rm -rf target/release/bundle/rpm
        npx tauri build --bundles rpm \
            --config "{\"bundle\":{\"linux\":{\"rpm\":{\"release\":\"$build\"}}}}" -- --locked
    ) || die "La compilation a échoué."

    shopt -s nullglob
    rpms=("$root"/target/release/bundle/rpm/*.rpm)
    shopt -u nullglob
    [[ ${#rpms[@]} -eq 1 ]] || die "La compilation devrait produire exactement un RPM (trouvé : ${#rpms[@]})."
    info "Installation de $(basename "${rpms[0]}")…"
    sudo dnf install -y "${rpms[0]}"
    ok "$APP_NAME $(rpm_version) installé (compilé localement)."
}

rpm_remove() {
    local name
    name=$(rpm_name)
    [[ -n $name ]] || { ok "Le RPM n'est pas installé : rien à supprimer."; return; }
    confirm "Supprimer le paquet $name $(rpm_version) ?" || { echo "Annulé."; return; }
    sudo dnf remove -y "$name"
    ok "RPM supprimé."
    remove_data "$HOME/.local/share/$APP_ID"
}

# ───────────────────────────── Flatpak ─────────────────────────────

# Version (« 0.2.0-12 ») du Flatpak produit par une exécution de $DEV_REPO : version
# de tauri.conf.json et nombre de commits au commit construit, comme dans la CI.
flatpak_build_version() {
    local sha=$1 count version
    count=$(gh api -i "repos/$DEV_REPO/commits?sha=$sha&per_page=1" 2>/dev/null \
        | grep -i '^link:' | sed -nE 's/.*[?&]page=([0-9]+)>; rel="last".*/\1/p')
    version=$(gh api "repos/$DEV_REPO/contents/src-tauri/tauri.conf.json?ref=$sha" --jq .content 2>/dev/null \
        | base64 -d 2>/dev/null | sed -nE 's/^ *"version": *"([^"]+)".*/\1/p' | head -n1)
    [[ -n $count && -n $version ]] && echo "$version-$count"
    return 0
}

flatpak_install() {
    command -v flatpak >/dev/null || die "flatpak introuvable : sudo dnf install flatpak"
    command -v python3 >/dev/null || die "python3 introuvable : sudo dnf install python3"

    local mode tag="" run="" sha="" new="" current source
    mode=$(choose "Quelle version du Flatpak ?" \
        "La plus récente (release publiée, sinon dernier build de $BRANCH)" \
        "Dernier build de $BRANCH (développement)" \
        "Dernière release publiée uniquement")
    if [[ $mode != 2 ]]; then
        tag=$(latest_release_with quayline.flatpak)
        if [[ -n $tag ]]; then
            new=${tag#v}
        elif [[ $mode == 3 ]]; then
            die "Aucune release publiée ne contient de Flatpak (essayez le dernier build)."
        else
            info "Aucune release avec Flatpak : on prend le dernier build de $BRANCH."
        fi
    fi
    if [[ -z $tag ]]; then
        need_gh
        read -r run sha < <(gh run list --repo "$DEV_REPO" --workflow "$FLATPAK_WORKFLOW" --branch "$BRANCH" \
            --status success --limit 1 --json databaseId,headSha --jq '.[0] | "\(.databaseId) \(.headSha)"')
        [[ -n ${run:-} && $run != null ]] || die "Aucun build Flatpak réussi trouvé sur $BRANCH ($DEV_REPO)."
        new=$(flatpak_build_version "$sha")
    fi
    current=$(flatpak_version)
    # Une release s'appelle « 1.4.1 », le Flatpak installé « 1.4.1-75 » (numéro de build).
    if [[ -n $new && ( $new == "$current" || -n $tag && $current == "$new"-* ) ]]; then
        ok "$APP_NAME $current (Flatpak) est déjà à jour."
        confirm "Réinstaller quand même ?" || return 0
    fi

    # Runtime GNOME : fourni par Flathub (téléchargé, rien n'y est publié).
    flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo

    tmp=$(mktemp -d)
    if [[ -n $tag ]]; then
        source="release $tag"
        info "Téléchargement du Flatpak ${new:-} ($source)…"
        release_download "$tag" "$tmp" quayline.flatpak SHA256SUMS.flatpak SHA256SUMS.flatpak.asc
    else
        source="build $run"
        info "Téléchargement du Flatpak ${new:-} ($source)…"
        gh run download "$run" --repo "$DEV_REPO" --name "$FLATPAK_ARTIFACT" --dir "$tmp"
    fi
    [[ -f $tmp/quayline.flatpak && -f $tmp/SHA256SUMS.flatpak ]] || die "Téléchargement incomplet."
    verify_signature "$tmp/SHA256SUMS.flatpak"
    (cd "$tmp" && sha256sum --quiet -c SHA256SUMS.flatpak) \
        || die "Empreinte SHA-256 incorrecte : paquet corrompu, installation annulée."
    info "Empreinte SHA-256 vérifiée."

    if [[ -n $current ]]; then
        info "Mise à jour : $current → ${new:-nouvelle version}…"
    else
        info "Installation…"
    fi
    flatpak install --user --reinstall --noninteractive "$tmp/quayline.flatpak"
    ok "$APP_NAME $(flatpak_version) (Flatpak) installé. Lancement : menu KDE, ou flatpak run $APP_ID"
}

flatpak_remove() {
    flatpak_installed || { ok "Le Flatpak n'est pas installé : rien à supprimer."; return; }
    confirm "Supprimer le Flatpak $APP_NAME $(flatpak_version) ?" || { echo "Annulé."; return; }
    flatpak uninstall --user --noninteractive "$APP_ID"
    # Le fichier .flatpak installe une source locale « …-origin » : on la retire aussi.
    local remote
    while read -r remote; do
        [[ -n $remote ]] && flatpak remote-delete --user --force "$remote" >/dev/null 2>&1 || true
    done < <(flatpak remotes --user --columns=name 2>/dev/null | grep -i "quayline" || true)
    ok "Flatpak supprimé."
    remove_data "$HOME/.var/app/$APP_ID"
}

# ──────────────────────────── Données ─────────────────────────────

# Le coffre n'est jamais supprimé sans confirmation explicite.
remove_data() {
    local dir=$1
    [[ -d $dir ]] || return 0
    echo
    echo "Le coffre et les réglages de cette version sont conservés dans :"
    echo "    $dir"
    echo "Ils resservent si vous réinstallez. Les supprimer efface définitivement vos"
    echo "serveurs, identifiants et clés enregistrés (sauf sauvegarde ailleurs)."
    confirm "Supprimer aussi ces données ?" || { ok "Données conservées."; return; }
    local answer
    read -r -p "Tapez SUPPRIMER pour confirmer : " answer
    if [[ $answer == "SUPPRIMER" ]]; then
        rm -rf -- "$dir"
        ok "Données supprimées."
    else
        ok "Données conservées."
    fi
}

# ─────────────────────────────── Menu ──────────────────────────────

[[ $EUID -ne 0 ]] || die "Lancez ce script sans sudo : il le demandera seulement si nécessaire."
command -v dnf >/dev/null || die "dnf introuvable : ce script est prévu pour Fedora."

show_state

format=$(choose "Quel format ?" "RPM (paquet système, dnf)" "Flatpak (application isolée)")
if [[ $format == 1 ]]; then
    label="RPM"; current=$(rpm_version)
else
    label="Flatpak"; current=$(flatpak_version)
    flatpak_installed && current=${current:-installé}
fi

if [[ -n $current ]]; then
    first="Mettre à jour (version installée : $current)"
else
    first="Installer"
fi
action=$(choose "$label — que faire ?" "$first" "Supprimer" "Quitter")

case "$format:$action" in
    1:1) rpm_install ;;
    1:2) rpm_remove ;;
    2:1) flatpak_install ;;
    2:2) flatpak_remove ;;
    *) echo "Rien à faire." ;;
esac
