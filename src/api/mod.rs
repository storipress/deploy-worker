use tracing::instrument;

mod client;
mod operation;
mod operations;

pub use client::Client;
pub use operations::ReleaseState;

use self::operations::{GetSite, GetSiteResponse};

#[instrument]
pub async fn update_release(client: &Client, state: operations::ReleaseState) {
    let release_id = &client.meta.release_id;
    if release_id.is_empty() {
        return;
    }

    if let Err(err) = update_release_inner(client, state).await {
        sentry_anyhow::capture_anyhow(&err);
    }
}

#[instrument]
async fn update_release_inner(
    client: &Client,
    state: operations::ReleaseState,
) -> anyhow::Result<()> {
    let op = operations::UpdateRelease::new(state);
    client.send(op).await?;
    Ok(())
}

#[instrument]
pub async fn get_site(client: &Client) -> anyhow::Result<Option<GetSiteResponse>> {
    client.send(GetSite::new()).await
}
