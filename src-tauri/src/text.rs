//! Textes venus d'un serveur (noms, messages, chemins) rendus inoffensifs à
//! l'affichage : un serveur ne doit pas pouvoir maquiller ce qu'il affiche.

/// Caractères invisibles ou qui réordonnent l'affichage : marques et contrôles
/// bidirectionnels (U+061C, U+200E/F, U+202A–E, U+2066–9), caractères de largeur nulle
/// (U+200B–D, U+FEFF) et séparateurs de ligne et de paragraphe (U+2028/9).
pub fn is_deceptive(c: char) -> bool {
    matches!(
        c,
        '\u{061C}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{FEFF}'
    )
}

/// Sans caractères de contrôle (sauf le saut de ligne) ni caractères trompeurs, et de
/// longueur bornée.
pub fn neutralize(text: &str, max_chars: usize) -> String {
    let cleaned: String = text
        .chars()
        .filter(|&c| (c == '\n' || !c.is_control()) && !is_deceptive(c))
        .take(max_chars)
        .collect();
    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deceptive_characters_are_removed() {
        assert_eq!(neutralize("facture\u{202E}fdp.exe", 80), "facturefdp.exe");
        assert_eq!(neutralize("a\u{200B}b\u{FEFF}c\u{061C}d\u{2028}e", 80), "abcde");
        assert_eq!(neutralize("ligne 1\nligne 2\x1b[31m", 80), "ligne 1\nligne 2[31m");
        assert_eq!(neutralize("abcdef", 3), "abc");
    }
}
