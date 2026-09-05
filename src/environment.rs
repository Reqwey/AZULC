pub const CURSEFORGE_API_KEY_ENV: &str = "AZULC_CURSEFORGE_API_KEY";
pub const MICROSOFT_CLIENT_ID_ENV: &str = "AZULC_MICROSOFT_CLIENT_ID";

const EMBEDDED_CURSEFORGE_API_KEY: &str = env!("AZULC_CURSEFORGE_API_KEY");
const EMBEDDED_MICROSOFT_CLIENT_ID: &str = env!("AZULC_MICROSOFT_CLIENT_ID");

pub fn curseforge_api_key() -> &'static str {
    EMBEDDED_CURSEFORGE_API_KEY.trim()
}

pub fn microsoft_client_id() -> &'static str {
    EMBEDDED_MICROSOFT_CLIENT_ID.trim()
}
