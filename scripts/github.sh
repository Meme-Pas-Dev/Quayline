# shellcheck shell=bash
# Accès à GitHub pour manage.sh et install-latest.sh (fichier inclus, pas un script).
#
# Les releases du dépôt public se téléchargent sans compte : avec curl, ou avec GitHub
# CLI (« gh ») s'il est installé et connecté. Les builds de développement (artifacts
# des workflows) exigent toujours gh connecté : GitHub ne les sert qu'aux comptes.
#
# Variables (facultatives) :
#     QUAYLINE_REPO      dépôt des releases (par défaut : Meme-Pas-Dev/quayline)
#     QUAYLINE_DEV_REPO  dépôt des builds de développement (par défaut : celui du
#                        clone qui contient ces scripts, sinon QUAYLINE_REPO)

REPO="${QUAYLINE_REPO:-Meme-Pas-Dev/quayline}"

# « propriétaire/dépôt » de l'origine GitHub du clone, ou rien.
clone_repo() {
    local url
    url=$(git -C "$SCRIPT_DIR/.." remote get-url origin 2>/dev/null) || return 0
    url=${url%/}
    url=${url%.git}
    case "$url" in
        https://github.com/*) url=${url#https://github.com/} ;;
        git@github.com:*) url=${url#git@github.com:} ;;
        ssh://git@github.com/*) url=${url#ssh://git@github.com/} ;;
        *) return 0 ;;
    esac
    [[ $url =~ ^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$ ]] && echo "$url"
    return 0
}

DEV_REPO="${QUAYLINE_DEV_REPO:-$(clone_repo)}"
DEV_REPO="${DEV_REPO:-$REPO}"

have_gh() {
    command -v gh >/dev/null && gh auth status >/dev/null 2>&1
}

need_gh() {
    have_gh && return 0
    command -v gh >/dev/null || die "Les builds de développement exigent GitHub CLI : sudo dnf install gh, puis : gh auth login"
    die "GitHub CLI n'est pas connecté. Lancez : gh auth login"
}

# Réponse JSON de l'API GitHub pour un chemin (« repos/… »).
api() {
    if have_gh; then
        gh api "$1"
    else
        command -v curl >/dev/null || die "curl introuvable : sudo dnf install curl"
        curl -fsSL --proto '=https' -H 'Accept: application/vnd.github+json' "https://api.github.com/$1"
    fi
}

# Extrait une valeur d'un document JSON lu sur l'entrée standard (expression Python
# appliquée à « d »), vide si absente.
json() {
    python3 -c "import json,sys
try:
    d = json.load(sys.stdin); v = $1
except Exception:
    v = None
print('' if v is None else v)"
}

# Étiquette de la dernière release publiée, ou rien.
latest_release_tag() {
    api "repos/$REPO/releases/latest" 2>/dev/null | json "d['tag_name']"
}

# Étiquette de la release la plus récente contenant le fichier $1, ou rien.
latest_release_with() {
    api "repos/$REPO/releases?per_page=30" 2>/dev/null \
        | json "next((r['tag_name'] for r in d if not r['draft'] and any(a['name'] == '$1' for a in r['assets'])), None)"
}

# Télécharge dans $2 les fichiers de la release $1 dont le nom correspond à l'un des
# motifs suivants (motifs du shell : '*.rpm', 'SHA256SUMS*'…).
release_download() {
    local tag=$1 dir=$2; shift 2
    if have_gh; then
        local args=() p
        for p in "$@"; do args+=(--pattern "$p"); done
        gh release download "$tag" --repo "$REPO" "${args[@]}" --dir "$dir"
        return
    fi
    local name url p found=0
    while IFS=$'\t' read -r name url; do
        for p in "$@"; do
            # shellcheck disable=SC2053 # motif voulu
            if [[ $name == $p ]]; then
                # Nom de fichier imposé par nous, jamais par la réponse : pas de « ../ ».
                [[ $name =~ ^[A-Za-z0-9._+-]+$ ]] || die "Nom de fichier inattendu dans la release : $name"
                curl -fsSL --proto '=https' -o "$dir/$name" "$url" || die "Téléchargement impossible : $name"
                found=1
                break
            fi
        done
    done < <(api "repos/$REPO/releases/tags/$tag" \
        | json "'\n'.join(a['name'] + '\t' + a['browser_download_url'] for a in d['assets'])")
    (( found )) || die "Aucun fichier attendu dans la release $tag."
}
