#!/usr/bin/env bash
# Télécharge et installe la dernière version de Quayline sur Fedora.
#
# Les releases se téléchargent sans compte GitHub. Le dernier build de développement
# (--dev) exige GitHub CLI connecté : sudo dnf install gh && gh auth login
#
# Utilisation :
#     scripts/install-latest.sh            # dernière release, sinon dernier build de main
#     scripts/install-latest.sh --dev      # dernier build réussi de main, même s'il existe une release
#     scripts/install-latest.sh --release  # uniquement la dernière release publiée
#     scripts/install-latest.sh --force    # réinstalle (ou revient en arrière) même si inutile

set -euo pipefail

WORKFLOW="rpm.yml"
BRANCH="main"
ARTIFACT="quayline-rpm-fc44"

mode="auto"
force=0
for arg in "$@"; do
    case "$arg" in
        --dev) mode="dev" ;;
        --release) mode="release" ;;
        --force) force=1 ;;
        -h | --help) sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "Option inconnue : $arg (voir --help)" >&2; exit 2 ;;
    esac
done

# Version installée, ou rien. « rpm -q » écrit « le paquet … n'est pas installé »
# sur la sortie standard : on ne garde sa réponse que s'il réussit.
installed_version() {
    local out
    out=$(rpm -q --queryformat '%{VERSION}-%{RELEASE}' "$1" 2>/dev/null) && printf '%s' "$out"
    return 0
}

die() { echo "✗ $*" >&2; exit 1; }
info() { echo "→ $*"; }

[[ $EUID -ne 0 ]] || die "Lancez ce script sans sudo : il le demandera seulement pour l'installation."
command -v dnf >/dev/null || die "dnf introuvable : ce script est prévu pour Fedora."
command -v python3 >/dev/null || die "python3 introuvable : sudo dnf install python3"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
# Suit les liens (~/.local/bin → clone) pour retrouver la clé et les autres scripts.
SCRIPT_DIR=$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)
# shellcheck source=scripts/github.sh
source "$SCRIPT_DIR/github.sh"

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

download_release() {
    local tag
    tag=$(latest_release_tag)
    [[ -n $tag ]] || return 1
    info "Téléchargement de la release $tag ($REPO)…"
    release_download "$tag" "$tmp" '*.rpm' 'SHA256SUMS' 'SHA256SUMS.asc'
}

download_dev() {
    need_gh
    local run
    run=$(gh run list --repo "$DEV_REPO" --workflow "$WORKFLOW" --branch "$BRANCH" --status success \
        --limit 1 --json databaseId --jq '.[0].databaseId // empty')
    [[ -n $run ]] || die "Aucun build réussi trouvé sur la branche $BRANCH de $DEV_REPO."
    info "Téléchargement du dernier build de $BRANCH ($DEV_REPO, exécution $run)…"
    gh run download "$run" --repo "$DEV_REPO" --name "$ARTIFACT" --dir "$tmp"
}

case "$mode" in
    release) download_release || die "Aucune release publiée pour l'instant (essayez --dev)." ;;
    dev) download_dev ;;
    auto) download_release || { info "Aucune release publiée : on prend le dernier build."; download_dev; } ;;
esac

shopt -s nullglob
rpms=("$tmp"/*.rpm)
[[ ${#rpms[@]} -eq 1 ]] || die "Le téléchargement devrait contenir exactement un RPM (trouvé : ${#rpms[@]})."
rpm_file=${rpms[0]}

[[ -f $tmp/SHA256SUMS ]] || die "Fichier SHA256SUMS absent : impossible de vérifier le paquet."
verify_signature "$tmp/SHA256SUMS"
(cd "$tmp" && sha256sum --quiet -c SHA256SUMS) || die "Empreinte SHA-256 incorrecte : paquet corrompu, installation annulée."
info "Empreinte SHA-256 vérifiée."

name=$(rpm -qp --queryformat '%{NAME}' "$rpm_file" 2>/dev/null)
new=$(rpm -qp --queryformat '%{VERSION}-%{RELEASE}' "$rpm_file" 2>/dev/null)
current=$(installed_version "$name")

if [[ $current == "$new" && $force -eq 0 ]]; then
    echo "✓ $name $new est déjà installé. (--force pour réinstaller)"
    exit 0
fi

newest=$(printf '%s\n%s\n' "$current" "$new" | sort -V | tail -1)

if [[ -z $current ]]; then
    info "Installation de $name $new…"
    sudo dnf install -y "$rpm_file"
elif [[ $current == "$new" ]]; then
    info "Réinstallation de $name $new…"
    sudo dnf reinstall -y "$rpm_file"
elif [[ $newest == "$current" ]]; then
    if [[ $force -eq 0 ]]; then
        echo "✓ La version installée ($current) est plus récente que $new : rien à faire. (--force pour revenir à $new)"
        exit 0
    fi
    info "Retour de $name $current à $new…"
    sudo dnf downgrade -y "$rpm_file"
else
    info "Mise à jour de $name : $current → $new…"
    sudo dnf install -y "$rpm_file"
fi

installed=$(installed_version "$name")
[[ $installed == "$new" ]] || die "$name $new n'a pas été installé (version actuelle : ${installed:-aucune})."
echo "✓ $name $new installé."
