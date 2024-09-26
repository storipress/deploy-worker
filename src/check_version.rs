use crate::{http::CLIENT, types::DeployMeta};
use backon::{ExponentialBuilder, Retryable};
use serde_derive::Deserialize;
use std::time::Duration;
use tokio::time;
use tracing::{instrument, warn};

#[derive(Debug, Clone)]
struct CheckVersion<'a> {
    url: String,
    release_id: &'a str,
}

#[derive(Deserialize)]
struct VersionResponse {
    rid: String,
}

impl<'a> CheckVersion<'a> {
    fn new(meta: &'a DeployMeta) -> Self {
        Self {
            url: format!("{}/api/_storipress/version", meta.cloudflare_page_url()),
            release_id: &meta.release_id,
        }
    }

    #[instrument]
    async fn check(&self) -> anyhow::Result<bool> {
        let res = CLIENT.get(&self.url).send().await?;
        let res: VersionResponse = res.json().await?;
        Ok(res.rid == self.release_id)
    }
}

#[instrument]
pub async fn wait_version_match(meta: &DeployMeta) -> anyhow::Result<()> {
    time::sleep(Duration::from_secs(1)).await;

    let checker = CheckVersion::new(meta);
    let do_check = || async {
        if checker.check().await? {
            Ok(())
        } else {
            Err(anyhow::anyhow!("version not match"))
        }
    };
    do_check
        .retry(
            ExponentialBuilder::default()
                .with_max_delay(Duration::from_secs(3))
                .with_max_times(2),
        )
        .notify(|err, _dur| {
            warn!(?err, "retry check version");
        })
        .await?;
    Ok(())
}
