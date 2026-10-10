//! Textes produits par le backend (messages d'erreur, page du navigateur, menus…),
//! dans la langue choisie par l'interface.
//!
//! Les catalogues sont ceux de l'interface (src/lib/i18n/en.json et fr.json), intégrés
//! au binaire : une seule source de textes, vérifiée par scripts/check-i18n.ts. Les
//! clés employées ici sont aussi contrôlées par un test (toutes présentes dans les
//! deux catalogues).

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::LazyLock;

const EN: u8 = 0;
const FR: u8 = 1;

static LANGUAGE: AtomicU8 = AtomicU8::new(EN);

type Catalog = HashMap<String, String>;

static CATALOGS: LazyLock<[Catalog; 2]> = LazyLock::new(|| {
    let parse = |raw: &str| serde_json::from_str::<Catalog>(raw).expect("catalogue de traduction invalide");
    [
        parse(include_str!("../../src/lib/i18n/en.json")),
        parse(include_str!("../../src/lib/i18n/fr.json")),
    ]
});

/// Langue choisie par l'interface (« en », « fr ») ; toute autre valeur donne l'anglais.
fn apply_language(language: &str) {
    LANGUAGE.store(if language == "fr" { FR } else { EN }, Ordering::Relaxed);
}

/// Code de la langue courante, pour l'attribut `lang` d'une page HTML.
pub fn language() -> &'static str {
    if LANGUAGE.load(Ordering::Relaxed) == FR { "fr" } else { "en" }
}

/// Texte de la clé dans la langue courante (anglais à défaut), paramètres « {nom} »
/// remplacés.
pub fn text(key: &str, params: &[(&str, &dyn Display)]) -> String {
    text_in(LANGUAGE.load(Ordering::Relaxed), key, params)
}

fn text_in(lang: u8, key: &str, params: &[(&str, &dyn Display)]) -> String {
    let catalogs = &*CATALOGS;
    let lang = lang as usize;
    let raw = catalogs[lang].get(key).or_else(|| catalogs[EN as usize].get(key)).map_or(key, String::as_str);
    let mut out = raw.to_string();
    for (name, value) in params {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

/// `tr!("clé")` ou `tr!("clé", nom = valeur, …)` : texte traduit.
macro_rules! tr {
    ($key:literal) => {
        $crate::i18n::text($key, &[])
    };
    // Bloc avec `let` : les emprunts des paramètres (non `Send`) sont libérés tout de
    // suite, même au milieu d'une expression qui contient un `.await`.
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {{
        let text = $crate::i18n::text($key, &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+]);
        text
    }};
}
pub(crate) use tr;

/// Reçoit la langue de l'interface, à son démarrage et à chaque changement.
#[tauri::command]
pub fn set_language(app: tauri::AppHandle, language: String) {
    apply_language(&language);
    // Le menu macOS est construit au démarrage, avant que la langue soit connue.
    #[cfg(target_os = "macos")]
    {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Ok(menu) = crate::platform::menu::build(&handle) {
                let _ = handle.set_menu(menu);
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Toutes les clés employées par `tr!` dans le backend existent dans chaque catalogue.
    #[test]
    fn backend_keys_exist() {
        let mut missing = Vec::new();
        let mut count = 0;
        for entry in walk(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")) {
            let src = std::fs::read_to_string(&entry).unwrap();
            for key in used_keys(&src) {
                count += 1;
                for (lang, catalog) in ["en", "fr"].iter().zip(CATALOGS.iter()) {
                    if !catalog.contains_key(key) {
                        missing.push(format!("{lang} : {key} ({})", entry.display()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "clés absentes :\n{}", missing.join("\n"));
        assert!(count > 0);
    }

    /// Clés littérales des appels `tr!("clé"…)`.
    fn used_keys(src: &str) -> Vec<&str> {
        src.match_indices("tr!(")
            // Pas `include_str!(`, ni un autre macro dont le nom finit par « tr ».
            .filter(|(i, _)| !src[..*i].ends_with(|c: char| c.is_alphanumeric() || c == '_'))
            .filter_map(|(i, _)| {
                let rest = src[i + 4..].trim_start().strip_prefix('"')?;
                rest.find('"').map(|end| &rest[..end])
            })
            .filter(|k| !k.contains(' ') && !k.is_empty() && *k != "clé")
            .collect()
    }

    fn walk(dir: std::path::PathBuf) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(path));
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn parameters_and_language() {
        assert_eq!(text_in(FR, "common.cancel", &[]), "Annuler");
        assert_eq!(text_in(FR, "hosts.noResults", &[("query", &"x")]), "Aucun résultat pour « x ».");
        assert_eq!(text_in(EN, "common.cancel", &[]), "Cancel");
        assert_eq!(text_in(EN, "clé.inconnue", &[]), "clé.inconnue");
    }
}
