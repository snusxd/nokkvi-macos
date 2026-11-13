//! Subsonic API client for `getAlbumInfo2`.
//!
//! Navidrome answers with the album's image links (`smallImageUrl`,
//! `mediumImageUrl`, `largeImageUrl`): public `/share/img/<token>` URLs that
//! open that one cover and carry no session credential. Discord Rich
//! Presence shows such a URL as the activity's image, which is how Feishin
//! gets its covers there.

use anyhow::Result;
use tracing::debug;

use crate::services::api::client::ApiClient;

/// Inner payload of the Subsonic `getAlbumInfo2` envelope
/// ([`crate::services::api::subsonic::SubsonicEnvelope`]).
#[derive(Debug, serde::Deserialize)]
struct AlbumInfoResponseInner {
    #[serde(rename = "albumInfo")]
    album_info: Option<AlbumInfoWire>,
}

/// The `albumInfo` object; Navidrome omits the fields it has nothing for.
#[derive(Debug, Default, serde::Deserialize)]
struct AlbumInfoWire {
    #[serde(rename = "smallImageUrl")]
    small_image_url: Option<String>,
    #[serde(rename = "mediumImageUrl")]
    medium_image_url: Option<String>,
    #[serde(rename = "largeImageUrl")]
    large_image_url: Option<String>,
}

impl AlbumInfoWire {
    /// The biggest link the server sent, an empty string counting as none.
    fn best_image_url(self) -> Option<String> {
        [
            self.large_image_url,
            self.medium_image_url,
            self.small_image_url,
        ]
        .into_iter()
        .flatten()
        .find(|url| !url.trim().is_empty())
    }
}

#[derive(Clone)]
pub struct AlbumInfoApiService {
    client: ApiClient,
    server_url: String,
    subsonic_credential: String,
}

impl AlbumInfoApiService {
    /// Create with a pre-authenticated ApiClient.
    pub fn new(client: ApiClient, server_url: String, subsonic_credential: String) -> Self {
        Self {
            client,
            server_url,
            subsonic_credential,
        }
    }

    /// The album's best image link (`getAlbumInfo2`), `None` when the server
    /// has none. A failed call (a rejected token, an id gone since a rescan)
    /// is an error, never a silent "no cover".
    pub async fn get_album_image_url(&self, album_id: &str) -> Result<Option<String>> {
        let inner: AlbumInfoResponseInner =
            crate::services::api::subsonic::subsonic_get_envelope_checked(
                &self.client.http_client(),
                &self.server_url,
                "getAlbumInfo2",
                &self.subsonic_credential,
                &[("id", album_id)],
                "getAlbumInfo2",
            )
            .await?;
        let url = inner.album_info.unwrap_or_default().best_image_url();
        debug!("getAlbumInfo2: id={album_id} image={}", url.is_some());
        Ok(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        services::api::subsonic::tests::{FAILED_ENVELOPE, NOT_FOUND_ENVELOPE, one_shot_server},
        types::error::NokkviError,
    };

    async fn url_from(base_url: String) -> Result<Option<String>> {
        let url = url::Url::parse(&base_url).expect("one-shot server url");
        AlbumInfoApiService::new(ApiClient::new(url, String::new()), base_url, "u=x".into())
            .get_album_image_url("al1")
            .await
    }

    /// The shape Feishin reads (`albumInfo.largeImageUrl`, falling back to the
    /// medium and small links); the `/share/img/` URLs match the artist
    /// answer recorded from Navidrome 0.64.2.
    #[tokio::test]
    async fn prefers_the_large_link_and_falls_back_to_smaller_ones() {
        const ALL: &str = r#"{"subsonic-response":{"status":"ok","version":"1.16.1","albumInfo":{"smallImageUrl":"https://m.example/share/img/t?size=300","mediumImageUrl":"https://m.example/share/img/t?size=600","largeImageUrl":"https://m.example/share/img/t?size=1200"}}}"#;
        assert_eq!(
            url_from(one_shot_server("200 OK", ALL).await)
                .await
                .expect("ok answer")
                .as_deref(),
            Some("https://m.example/share/img/t?size=1200")
        );

        const MEDIUM_ONLY: &str = r#"{"subsonic-response":{"status":"ok","version":"1.16.1","albumInfo":{"smallImageUrl":"","mediumImageUrl":"https://m.example/share/img/t?size=600","largeImageUrl":""}}}"#;
        assert_eq!(
            url_from(one_shot_server("200 OK", MEDIUM_ONLY).await)
                .await
                .expect("ok answer")
                .as_deref(),
            Some("https://m.example/share/img/t?size=600")
        );
    }

    #[tokio::test]
    async fn an_album_with_no_links_has_no_image() {
        const EMPTY: &str =
            r#"{"subsonic-response":{"status":"ok","version":"1.16.1","albumInfo":{}}}"#;
        assert_eq!(
            url_from(one_shot_server("200 OK", EMPTY).await)
                .await
                .expect("ok answer"),
            None
        );
    }

    #[tokio::test]
    async fn a_failed_envelope_is_an_error() {
        let err = url_from(one_shot_server("200 OK", NOT_FOUND_ENVELOPE).await)
            .await
            .expect_err("a failed envelope must be an error");
        assert!(
            format!("{err:#}").contains("data not found"),
            "got: {err:#}"
        );

        let err = url_from(one_shot_server("200 OK", FAILED_ENVELOPE).await)
            .await
            .expect_err("a rejected token must be an error");
        assert!(matches!(
            err.downcast_ref::<NokkviError>(),
            Some(NokkviError::Unauthorized)
        ));
    }
}
