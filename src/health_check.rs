use crate::{constants::SENTRY_CRON_CHECK_URL, http::build_client};
use once_cell::sync::Lazy;
use reqwest::StatusCode;
use reqwest_middleware::ClientWithMiddleware;
use std::time::Instant;
use tracing::{debug, error, instrument, warn};

static CLIENT: Lazy<ClientWithMiddleware> =
    Lazy::new(|| build_client(|builder| builder.gzip(true)));

#[derive(Debug)]
pub struct HealthCheckInner(Instant);

#[instrument]
pub async fn start_job(check_in_id: &str) -> Option<HealthCheckInner> {
    match CLIENT
        .get(SENTRY_CRON_CHECK_URL)
        .query(&[("status", "in_progress"), ("check_in_id", check_in_id)])
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            if status != StatusCode::ACCEPTED {
                warn!(?status, "Unexpected status for check in request");
            }
            Some(HealthCheckInner(Instant::now()))
        }
        Err(err) => {
            error!(?err, "Fail to send check in request");
            None
        }
    }
}

#[instrument]
pub async fn end_job(check_in_id: &str, HealthCheckInner(instant): HealthCheckInner) {
    let duration = instant.elapsed();
    debug!(?duration, "finish check in {check_in_id}");
    match CLIENT
        .get(SENTRY_CRON_CHECK_URL)
        .query(&[("status", "ok"), ("check_in_id", check_in_id)])
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            if status != StatusCode::ACCEPTED {
                warn!(?status, "Unexpected status for check in request");
            }
        }
        Err(err) => {
            error!(?err, "Fail to send finish check in");
        }
    }
}

#[must_use = "Must call finish to report success"]
#[derive(Debug)]
pub struct HealthCheck(String, Option<HealthCheckInner>);

impl HealthCheck {
    #[instrument]
    pub async fn start() -> Self {
        let check_in_id = uuid::Uuid::new_v4().to_string();
        let inner = start_job(&check_in_id).await;
        debug!("start check in {:?}", inner.as_ref().map(|inner| &inner.0));
        Self(check_in_id, inner)
    }

    #[instrument]
    pub async fn finish(self) {
        let HealthCheck(check_in_id, Some(inner)) = self else {
            debug!("finish check in without id");
            return;
        };

        end_job(&check_in_id, inner).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_check_client_smoke_test() {
        Lazy::force(&CLIENT);
    }
}
