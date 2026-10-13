//! Encodages des fichiers texte ouverts dans l'éditeur intégré : UTF-8 (avec ou sans
//! BOM), UTF-16 (petit ou grand boutiste) et Windows-1252 (« ANSI » des anciens
//! fichiers Windows). Le fichier est réenregistré dans son encodage d'origine.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Charset {
    #[serde(rename = "utf-8")]
    Utf8,
    #[serde(rename = "utf-16le")]
    Utf16Le,
    #[serde(rename = "utf-16be")]
    Utf16Be,
    #[serde(rename = "windows-1252")]
    Windows1252,
}

/// Encodage d'un fichier, et présence d'une marque d'ordre des octets (BOM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Encoding {
    pub charset: Charset,
    pub bom: bool,
}

/// Octets 0x80 à 0x9F de Windows-1252 ; les cinq positions non définies gardent le
/// caractère de contrôle de même code (comme le standard WHATWG), pour un aller-retour
/// sans perte.
const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

/// Contenu binaire : octet nul, ou trop de caractères de contrôle.
fn looks_binary(data: &[u8]) -> bool {
    let sample = &data[..data.len().min(8192)];
    if sample.contains(&0) {
        return true;
    }
    let controls = sample.iter().filter(|&&b| b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r' | 0x0c | 0x1b)).count();
    controls * 100 > sample.len()
}

/// UTF-16 petit boutiste sans BOM : un octet nul sur deux, toujours à la même place.
fn looks_utf16le(data: &[u8]) -> bool {
    let sample = &data[..data.len().min(8192) & !1];
    if sample.len() < 4 {
        return false;
    }
    let pairs = sample.len() / 2;
    let high_zero = sample.chunks(2).filter(|p| p[1] == 0).count();
    let low_zero = sample.chunks(2).filter(|p| p[0] == 0).count();
    high_zero * 10 >= pairs * 9 && low_zero == 0
}

fn decode_utf16(data: &[u8], little: bool) -> Option<String> {
    if !data.len().is_multiple_of(2) {
        return None;
    }
    let units = data.chunks(2).map(|p| if little { u16::from_le_bytes([p[0], p[1]]) } else { u16::from_be_bytes([p[0], p[1]]) });
    char::decode_utf16(units).collect::<Result<String, _>>().ok()
}

/// Texte et encodage d'un fichier ; `None` s'il n'est pas du texte.
pub fn decode(data: &[u8]) -> Option<(String, Encoding)> {
    let enc = |charset, bom| Encoding { charset, bom };
    if let Some(rest) = data.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        let text = std::str::from_utf8(rest).ok()?;
        return Some((text.to_owned(), enc(Charset::Utf8, true)));
    }
    if let Some(rest) = data.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(rest, true).map(|t| (t, enc(Charset::Utf16Le, true)));
    }
    if let Some(rest) = data.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(rest, false).map(|t| (t, enc(Charset::Utf16Be, true)));
    }
    if looks_utf16le(data) {
        return decode_utf16(data, true).map(|t| (t, enc(Charset::Utf16Le, false)));
    }
    if looks_binary(data) {
        return None;
    }
    if let Ok(text) = std::str::from_utf8(data) {
        return Some((text.to_owned(), enc(Charset::Utf8, false)));
    }
    // Ni UTF-8 ni binaire : ancien fichier Windows (ou Latin-1, dont Windows-1252 est
    // un sur-ensemble pour tous les caractères imprimables).
    let text = data.iter().map(|&b| if (0x80..0xA0).contains(&b) { CP1252_HIGH[usize::from(b - 0x80)] } else { char::from(b) }).collect();
    Some((text, enc(Charset::Windows1252, false)))
}

/// Octets du texte dans `encoding`. `Err` avec le premier caractère qui n'existe pas
/// dans l'encodage (Windows-1252 seulement).
pub fn encode(text: &str, encoding: Encoding) -> Result<Vec<u8>, char> {
    let mut out = Vec::with_capacity(text.len() + 3);
    match encoding.charset {
        Charset::Utf8 => {
            if encoding.bom {
                out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            }
            out.extend_from_slice(text.as_bytes());
        }
        Charset::Utf16Le | Charset::Utf16Be => {
            let little = encoding.charset == Charset::Utf16Le;
            if encoding.bom {
                out.extend_from_slice(if little { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] });
            }
            for unit in text.encode_utf16() {
                out.extend_from_slice(&if little { unit.to_le_bytes() } else { unit.to_be_bytes() });
            }
        }
        Charset::Windows1252 => {
            for c in text.chars() {
                let byte = match u32::from(c) {
                    b @ (0..=0x7F | 0xA0..=0xFF) => b as u8,
                    _ => match CP1252_HIGH.iter().position(|&x| x == c) {
                        Some(i) => 0x80 + i as u8,
                        None => return Err(c),
                    },
                };
                out.push(byte);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(data: &[u8], charset: Charset, bom: bool) {
        let (text, enc) = decode(data).expect("texte");
        assert_eq!(enc, Encoding { charset, bom });
        assert_eq!(encode(&text, enc).unwrap(), data, "aller-retour sans perte");
    }

    #[test]
    fn encodings_are_detected_and_preserved() {
        round_trip("échelle\n".as_bytes(), Charset::Utf8, false);
        round_trip(b"\xEF\xBB\xBFpasse\n", Charset::Utf8, true);
        // « @echo off » + « Fenêtre » en Windows-1252 (ê = 0xEA, € = 0x80).
        round_trip(b"@echo off\r\nrem Fen\xEAtre \x80\r\n", Charset::Windows1252, false);
        let utf16: Vec<u8> = [0xFF, 0xFE].into_iter().chain("Clé=1\r\n".encode_utf16().flat_map(u16::to_le_bytes)).collect();
        round_trip(&utf16, Charset::Utf16Le, true);
        let utf16_nobom: Vec<u8> = "log ok\r\n".encode_utf16().flat_map(u16::to_le_bytes).collect();
        round_trip(&utf16_nobom, Charset::Utf16Le, false);
        let be: Vec<u8> = [0xFE, 0xFF].into_iter().chain("été".encode_utf16().flat_map(u16::to_be_bytes)).collect();
        round_trip(&be, Charset::Utf16Be, true);
    }

    #[test]
    fn binary_is_refused_and_unencodable_reported() {
        assert!(decode(b"\x7fELF\x02\x01\x01\0\0\0").is_none());
        assert!(decode(&[0x01, 0x02, 0x03, 0x04, b'a']).is_none());
        let latin = Encoding { charset: Charset::Windows1252, bom: false };
        assert_eq!(encode("prix : 5 €", latin).unwrap().last(), Some(&0x80));
        assert_eq!(encode("smile 😀", latin), Err('😀'));
    }
}
