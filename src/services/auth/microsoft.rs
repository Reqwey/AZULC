//! Microsoft device-code authentication and Minecraft profile retrieval.

use crate::domain::{AccountProvider, OfflineAccount};
use image::{RgbaImage, imageops::FilterType};
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use serde_json::json;
use std::{
    env, fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub const CLIENT_ID_ENV: &str = "AZULC_MICROSOFT_CLIENT_ID";
const SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE_ENDPOINT: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_ENDPOINT: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBL_ENDPOINT: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_ENDPOINT: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MINECRAFT_TOKEN_ENDPOINT: &str =
    "https://api.minecraftservices.com/authentication/login_with_xbox";
const PROFILE_ENDPOINT: &str = "https://api.minecraftservices.com/minecraft/profile";

pub fn is_configured() -> bool {
    client_id().is_ok()
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceAuthorization {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    #[serde(default = "default_interval")]
    pub interval: u64,
    pub expires_in: u64,
}

impl DeviceAuthorization {
    pub fn verification_url(&self) -> &str {
        self.verification_uri_complete
            .as_deref()
            .unwrap_or(&self.verification_uri)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MicrosoftError {
    #[error("set {CLIENT_ID_ENV} to the client ID of the approved AZULC Azure application")]
    MissingClientId,
    #[error("Microsoft sign-in was cancelled")]
    Cancelled,
    #[error("Microsoft sign-in code expired; start sign-in again")]
    Expired,
    #[error("Microsoft sign-in was denied")]
    AccessDenied,
    #[error("this Microsoft account does not own a Minecraft Java profile")]
    NoMinecraftProfile,
    #[error("Microsoft authentication request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Microsoft authentication returned {status}: {message}")]
    Http { status: StatusCode, message: String },
    #[error("Microsoft authentication response was incomplete: {0}")]
    InvalidResponse(&'static str),
    #[error("the active Minecraft skin URL was not trusted")]
    UntrustedSkinUrl,
    #[error("could not decode the active Minecraft skin: {0}")]
    Skin(#[from] image::ImageError),
}

#[derive(Clone)]
pub struct AccountRefreshError {
    message: String,
    replacement_refresh_token: Option<String>,
}

impl AccountRefreshError {
    fn before_token_rotation(source: MicrosoftError) -> Self {
        Self {
            message: source.to_string(),
            replacement_refresh_token: None,
        }
    }

    fn after_token_rotation(source: MicrosoftError, refresh_token: String) -> Self {
        Self {
            message: source.to_string(),
            replacement_refresh_token: Some(refresh_token),
        }
    }

    pub fn into_parts(self) -> (String, Option<String>) {
        (self.message, self.replacement_refresh_token)
    }
}

impl fmt::Debug for AccountRefreshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccountRefreshError")
            .field("message", &self.message)
            .field(
                "replacement_refresh_token",
                &self
                    .replacement_refresh_token
                    .as_ref()
                    .map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl fmt::Display for AccountRefreshError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AccountRefreshError {}

#[derive(Debug, Deserialize)]
struct OAuthTokens {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct OAuthFailure {
    error: String,
    #[serde(default)]
    error_description: String,
}

#[derive(Debug, Deserialize)]
struct XboxToken {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XboxClaims,
}

#[derive(Debug, Deserialize)]
struct XboxClaims {
    xui: Vec<XboxUser>,
}

#[derive(Debug, Deserialize)]
struct XboxUser {
    uhs: String,
    #[serde(default)]
    xid: String,
}

#[derive(Debug, Deserialize)]
struct MinecraftToken {
    access_token: String,
    #[serde(default = "default_minecraft_expiry")]
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct MinecraftProfile {
    id: String,
    name: String,
    #[serde(default)]
    skins: Vec<MinecraftTexture>,
}

#[derive(Debug, Deserialize)]
struct MinecraftTexture {
    state: String,
    url: String,
}

pub async fn begin_device_authorization() -> Result<DeviceAuthorization, MicrosoftError> {
    let client_id = client_id()?;
    let client = http_client()?;
    let response = client
        .post(DEVICE_ENDPOINT)
        .form(&[("client_id", client_id.as_str()), ("scope", SCOPE)])
        .send()
        .await?;
    decode_json(response).await
}

pub async fn complete_device_authorization(
    authorization: DeviceAuthorization,
    cancelled: Arc<AtomicBool>,
) -> Result<OfflineAccount, MicrosoftError> {
    let client_id = client_id()?;
    let client = http_client()?;
    let started = std::time::Instant::now();
    let mut interval = authorization.interval.max(1);
    let oauth = loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(MicrosoftError::Cancelled);
        }
        let response = client
            .post(TOKEN_ENDPOINT)
            .form(&[
                ("client_id", client_id.as_str()),
                ("device_code", authorization.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await?;
        if response.status().is_success() {
            break response.json::<OAuthTokens>().await?;
        }
        let status = response.status();
        let failure = response
            .json::<OAuthFailure>()
            .await
            .map_err(MicrosoftError::Network)?;
        match failure.error.as_str() {
            "authorization_pending" => {}
            "slow_down" => interval = interval.saturating_add(5),
            "access_denied" => return Err(MicrosoftError::AccessDenied),
            "expired_token" => return Err(MicrosoftError::Expired),
            _ => {
                return Err(MicrosoftError::Http {
                    status,
                    message: failure.error_description,
                });
            }
        }
        if started.elapsed().as_secs() >= authorization.expires_in {
            return Err(MicrosoftError::Expired);
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    };
    account_from_oauth(&client, oauth).await
}

pub async fn refresh_account(
    account: &OfflineAccount,
) -> Result<OfflineAccount, AccountRefreshError> {
    let client_id = client_id().map_err(AccountRefreshError::before_token_rotation)?;
    let refresh_token = account
        .refresh_token
        .as_deref()
        .filter(|token| !token.is_empty())
        .ok_or(MicrosoftError::Expired)
        .map_err(AccountRefreshError::before_token_rotation)?;
    let client = http_client().map_err(AccountRefreshError::before_token_rotation)?;
    let response = client
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("client_id", client_id.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
            ("scope", SCOPE),
        ])
        .send()
        .await
        .map_err(MicrosoftError::Network)
        .map_err(AccountRefreshError::before_token_rotation)?;
    let mut oauth: OAuthTokens = decode_json(response)
        .await
        .map_err(AccountRefreshError::before_token_rotation)?;
    if oauth.refresh_token.is_empty() {
        oauth.refresh_token = refresh_token.to_owned();
    }
    let replacement_refresh_token = oauth.refresh_token.clone();
    account_from_oauth(&client, oauth).await.map_err(|source| {
        AccountRefreshError::after_token_rotation(source, replacement_refresh_token)
    })
}

async fn account_from_oauth(
    client: &Client,
    oauth: OAuthTokens,
) -> Result<OfflineAccount, MicrosoftError> {
    let xbl: XboxToken = decode_json(
        client
            .post(XBL_ENDPOINT)
            .json(&json!({
                "Properties": {
                    "AuthMethod": "RPS",
                    "SiteName": "user.auth.xboxlive.com",
                    "RpsTicket": format!("d={}", oauth.access_token)
                },
                "RelyingParty": "http://auth.xboxlive.com",
                "TokenType": "JWT"
            }))
            .send()
            .await?,
    )
    .await?;

    let xsts: XboxToken = decode_json(
        client
            .post(XSTS_ENDPOINT)
            .json(&json!({
                "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl.token] },
                "RelyingParty": "rp://api.minecraftservices.com/",
                "TokenType": "JWT"
            }))
            .send()
            .await?,
    )
    .await?;
    let user = xsts
        .display_claims
        .xui
        .first()
        .ok_or(MicrosoftError::InvalidResponse("XSTS user claim"))?;

    let minecraft: MinecraftToken = decode_json(
        client
            .post(MINECRAFT_TOKEN_ENDPOINT)
            .json(&json!({
                "identityToken": format!("XBL3.0 x={};{}", user.uhs, xsts.token)
            }))
            .send()
            .await?,
    )
    .await?;
    let profile_response = client
        .get(PROFILE_ENDPOINT)
        .bearer_auth(&minecraft.access_token)
        .send()
        .await?;
    if profile_response.status() == StatusCode::NOT_FOUND {
        return Err(MicrosoftError::NoMinecraftProfile);
    }
    let profile: MinecraftProfile = decode_json(profile_response).await?;
    let avatar_rgba = match profile
        .skins
        .iter()
        .find(|skin| skin.state.eq_ignore_ascii_case("ACTIVE"))
    {
        // A transient texture-CDN failure must not invalidate an otherwise
        // authenticated Minecraft account. The UI has a local fallback icon.
        Some(skin) => fetch_avatar(client, &skin.url).await.ok(),
        None => None,
    };
    let uuid = Uuid::parse_str(&profile.id)
        .map_err(|_| MicrosoftError::InvalidResponse("Minecraft profile UUID"))?;
    Ok(OfflineAccount {
        username: profile.name,
        uuid,
        provider: AccountProvider::Microsoft,
        access_token: Some(minecraft.access_token),
        refresh_token: Some(oauth.refresh_token),
        token_expires_at: Some(now_unix().saturating_add(minecraft.expires_in)),
        xuid: (!user.xid.is_empty()).then(|| user.xid.clone()),
        avatar_rgba,
    })
}

async fn fetch_avatar(client: &Client, value: &str) -> Result<Vec<u8>, MicrosoftError> {
    let url = Url::parse(value).map_err(|_| MicrosoftError::UntrustedSkinUrl)?;
    if url.scheme() != "https" || url.host_str() != Some("textures.minecraft.net") {
        return Err(MicrosoftError::UntrustedSkinUrl);
    }
    let bytes = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let skin = image::load_from_memory(&bytes)?.to_rgba8();
    render_avatar(&skin).ok_or(MicrosoftError::InvalidResponse("Minecraft skin dimensions"))
}

fn render_avatar(skin: &RgbaImage) -> Option<Vec<u8>> {
    let scale = skin.width() / 64;
    if scale == 0 || skin.height() < 16 * scale {
        return None;
    }
    let face =
        image::imageops::crop_imm(skin, 8 * scale, 8 * scale, 8 * scale, 8 * scale).to_image();
    let hat =
        image::imageops::crop_imm(skin, 40 * scale, 8 * scale, 8 * scale, 8 * scale).to_image();
    let mut avatar = image::imageops::resize(&face, 64, 64, FilterType::Nearest);
    let hat = image::imageops::resize(&hat, 64, 64, FilterType::Nearest);
    image::imageops::overlay(&mut avatar, &hat, 0, 0);
    Some(avatar.into_raw())
}

async fn decode_json<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> Result<T, MicrosoftError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response.json().await?);
    }
    let message = response.text().await.unwrap_or_default();
    Err(MicrosoftError::Http {
        status,
        message: message.chars().take(500).collect(),
    })
}

fn client_id() -> Result<String, MicrosoftError> {
    env::var(CLIENT_ID_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_owned())
        .ok_or(MicrosoftError::MissingClientId)
}

fn http_client() -> Result<Client, MicrosoftError> {
    Ok(Client::builder()
        .user_agent(concat!("AZULC/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(45))
        .build()?)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

const fn default_interval() -> u64 {
    5
}

const fn default_minecraft_expiry() -> u64 {
    86_400
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_error_debug_output_redacts_a_rotated_token() {
        let error = AccountRefreshError::after_token_rotation(
            MicrosoftError::Expired,
            "top-secret-token".into(),
        );

        let debug = format!("{error:?}");

        assert!(!debug.contains("top-secret-token"));
    }

    #[test]
    fn renders_face_and_hat_from_a_standard_skin() {
        let mut skin = RgbaImage::new(64, 64);
        for y in 8..16 {
            for x in 8..16 {
                skin.put_pixel(x, y, image::Rgba([10, 20, 30, 255]));
            }
        }
        skin.put_pixel(40, 8, image::Rgba([200, 100, 50, 255]));
        let avatar = render_avatar(&skin).unwrap();
        assert_eq!(avatar.len(), 64 * 64 * 4);
        assert_eq!(&avatar[..4], &[200, 100, 50, 255]);
        assert_eq!(&avatar[(63 * 4)..(64 * 4)], &[10, 20, 30, 255]);
    }
}
