//! Lecture bornée des réponses HTTP (fournisseur d'identité, GitHub) : un serveur ne
//! doit pas pouvoir faire grossir la mémoire sans limite, même sans `Content-Length`.

use serde::de::DeserializeOwned;

/// Largement assez pour un document de découverte, un jeu de clés ou un jeton.
pub const MAX_JSON: usize = 1024 * 1024;

/// Corps de la réponse, refusé au-delà de `max` octets.
pub async fn body(mut response: reqwest::Response, max: usize) -> Result<Vec<u8>, String> {
    let too_large = || format!("response larger than {max} bytes");
    if response.content_length().is_some_and(|l| l > max as u64) {
        return Err(too_large());
    }
    let mut out = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if out.len() + chunk.len() > max {
            return Err(too_large());
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// Corps JSON de la réponse, borné à [`MAX_JSON`].
pub async fn json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, String> {
    serde_json::from_slice(&body(response, MAX_JSON).await?).map_err(|e| e.to_string())
}
