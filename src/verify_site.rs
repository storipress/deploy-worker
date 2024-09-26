use crate::{
    api::{get_site, Client},
    http::CLIENT,
};
use once_cell::sync::Lazy;
use reqwest::StatusCode;
use std::{collections::BTreeMap, time::Duration};
use tokio::{sync::Semaphore, time::sleep};
use tracing::{debug, error, info, instrument, warn};

#[instrument]
pub async fn verify_site(client: Client) {
    // delay 5 second for start checking
    sleep(Duration::from_secs(5)).await;

    if let Err(err) = verify_site_immediate(&client).await {
        sentry_anyhow::capture_anyhow(&err);
    }
}

#[instrument(err)]
pub async fn verify_site_immediate(client: &Client) -> anyhow::Result<()> {
    info!(?client.meta, "start verify site");

    let res = get_site(&client).await;
    let site = match res {
        Ok(Some(site)) => site,
        Ok(None) => {
            warn!(?client.meta, "get site return empty response");
            sentry::with_scope(
                |scope| {
                    scope.set_user(Some(sentry::User {
                        id: Some(client.meta.client_id.clone()),
                        ..Default::default()
                    }))
                },
                || {
                    sentry::capture_message(
                        "get site return empty response",
                        sentry::Level::Warning,
                    );
                },
            );
            return Ok(());
        }
        Err(err) => {
            error!(?err, ?client.meta, "get site error");
            return Ok(());
        }
    };

    let storipress_url = site.customer_site_storipress_url();
    let url = format!("https://{storipress_url}");

    verify_site_scripts(&client, &url).await
}

#[instrument(err)]
async fn verify_site_scripts(client: &Client, url: &str) -> anyhow::Result<()> {
    static MAX_CONCURRENT_REQUESTS: Semaphore = Semaphore::const_new(2);
    let _permit = MAX_CONCURRENT_REQUESTS.acquire().await?;
    let res = CLIENT.get(url).send().await?;
    let html = res.text().await?;
    let scripts = extract_scripts(html);

    for src in scripts.iter() {
        let script_url = format!("{url}{src}");
        // fetch script
        let res = CLIENT.get(&script_url).send().await?;
        let status = res.status();
        debug!(?status, ?script_url, "checking script");
        // if response is 404, it means that site cache is broken
        if status == StatusCode::NOT_FOUND {
            // this will convert to sentry capture message
            warn!(?client.meta, "detect site cache issue");

            sentry::with_scope(
                |scope| {
                    let mut other = BTreeMap::new();
                    other.insert("url".to_owned(), url.to_owned().into());

                    scope.set_user(Some(sentry::User {
                        id: Some(client.meta.client_id.clone()),
                        other,
                        ..Default::default()
                    }))
                },
                || {
                    sentry::capture_message("detect site cache issue", sentry::Level::Warning);
                },
            );
            return Ok(());
        }
    }
    info!(?client.meta, "site look good");
    Ok(())
}

static SCRIPT_SELECTOR: Lazy<scraper::Selector> = Lazy::new(|| {
    scraper::Selector::parse("script[type=module]").expect("script selector parse error")
});

#[instrument(ret, skip(html), fields(html_length = html.len()))]
fn extract_scripts(html: String) -> Vec<String> {
    // These functions are not thread-safe, so it must not live in async context
    let html = scraper::Html::parse_document(&html);

    let mut scripts = vec![];

    for script in html.select(&SCRIPT_SELECTOR) {
        let src = script.value().attr("src");
        debug!(?src, "found script");
        if let Some(src) = src {
            if src.starts_with("/_nuxt/") {
                scripts.push(src.to_owned());
            }
        }
    }

    if scripts.is_empty() {
        warn!("no script found");
    }

    scripts
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::{prelude::*, EnvFilter};

    use super::*;
    use crate::types::DeployMeta;

    #[test]
    fn test_selector_work() {
        Lazy::force(&super::SCRIPT_SELECTOR);
    }
}
