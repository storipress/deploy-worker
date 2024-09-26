use serde_derive::Deserialize;
use strum::AsRefStr;
use tracing::warn;

#[derive(Debug, Copy, Clone)]
pub struct FileSummary {
    pub ignored: i32,
    pub removed_fail: i32,
    pub removed_success: i32,
}

impl FileSummary {
    pub fn total(&self) -> i32 {
        self.ignored + self.removed_fail + self.removed_success
    }
}

#[derive(Deserialize, Debug, AsRefStr, Default, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "snake_case", from = "String")]
pub enum DeployType {
    #[default]
    Static,
    CloudflareFunction,
}

impl From<String> for DeployType {
    fn from(value: String) -> Self {
        match value.as_str() {
            "static" => DeployType::Static,
            "cloudflare_function" => DeployType::CloudflareFunction,
            _ => {
                warn!("unknown deploy type {value}, default to static");
                DeployType::Static
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ClientType {
    Production,
    Staging,
    Development,
    Unknown,
}

impl ClientType {
    #[inline]
    pub fn api_base(&self) -> Option<&'static str> {
        let base = match self {
            ClientType::Production => "https://api.stori.press",
            ClientType::Staging => "https://api.storipress.pro",
            ClientType::Development => "https://api.storipress.dev",
            ClientType::Unknown => return None,
        };
        Some(base)
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct DeployMeta {
    pub page_id: String,
    pub client_id: String,
    pub release_id: String,

    pub source: Option<String>,
    pub output_path: Option<String>,
    pub token: Option<String>,
    #[serde(default)]
    pub deploy_type: DeployType,

    #[cfg(feature = "intended_fail")]
    #[serde(default)]
    pub __storipress_deployer_force_error: bool,
}

impl DeployMeta {
    pub fn derive_deploy_type_from_source(&mut self) {
        if matches!(
            self.source.as_deref(),
            Some("generator-next" | "generator-v2")
        ) {
            self.deploy_type = DeployType::CloudflareFunction;
        }
    }

    #[inline]
    pub fn client_type(&self) -> ClientType {
        match self.client_id.chars().next() {
            Some('P') => ClientType::Production,
            Some('S') => ClientType::Staging,
            Some('D') => ClientType::Development,
            _ => ClientType::Unknown,
        }
    }

    pub fn token(&self) -> &str {
        self.token.as_deref().unwrap_or("")
    }

    #[inline]
    pub fn is_static(&self) -> bool {
        self.deploy_type == DeployType::Static
    }

    pub fn api_host(&self) -> anyhow::Result<String> {
        let client_id = &self.client_id;

        let host = self
            .client_type()
            .api_base()
            .ok_or_else(|| anyhow::anyhow!("Fail to create api host url"))?;

        Ok(format!("{host}/client/{client_id}/graphql"))
    }

    pub fn cloudflare_page_url(&self) -> String {
        format!(
            "https://{}.{}.pages.dev",
            self.client_id.to_lowercase(),
            self.page_id
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_deploy_type() {
        let mut meta = DeployMeta {
            source: Some("generator-next".to_owned()),
            ..Default::default()
        };

        meta.derive_deploy_type_from_source();

        assert!(meta.deploy_type == DeployType::CloudflareFunction);

        let mut meta = DeployMeta {
            source: Some("generator-v2".to_owned()),
            ..Default::default()
        };

        meta.derive_deploy_type_from_source();

        assert!(meta.deploy_type == DeployType::CloudflareFunction);

        let mut meta = DeployMeta {
            source: Some("generator".to_owned()),
            ..Default::default()
        };

        meta.derive_deploy_type_from_source();

        assert!(meta.deploy_type == DeployType::Static);

        let mut meta = DeployMeta {
            source: Some("generator".to_owned()),
            deploy_type: DeployType::CloudflareFunction,
            ..Default::default()
        };

        meta.derive_deploy_type_from_source();

        assert!(meta.deploy_type == DeployType::CloudflareFunction);
    }
}
