use aws_sdk_cloudwatch::{
    types::{Dimension, MetricDatum, StandardUnit},
    Client,
};
use std::time::Instant;
use tracing::error;

use crate::types::{DeployMeta, FileSummary};

pub fn start<'a>(client: &'a Client) -> DurationMetricGuard<'a> {
    DurationMetricGuard::new(client)
}

#[must_use]
pub struct DurationMetricGuard<'a> {
    client: &'a Client,
    instant: Instant,
}

impl<'a> DurationMetricGuard<'a> {
    pub fn new(client: &'a Client) -> Self {
        Self {
            client,
            instant: Instant::now(),
        }
    }

    pub async fn stop(self, meta: &DeployMeta, file: &FileSummary) {
        let duration = self.instant.elapsed();
        if let Err(err) = self
            .client
            .put_metric_data()
            .namespace("Deployer")
            .metric_data(
                MetricDatum::builder()
                    .metric_name("duration")
                    .value(duration.as_millis() as f64)
                    .unit(StandardUnit::Milliseconds)
                    .dimensions(
                        Dimension::builder()
                            .name("page_id")
                            .value(&meta.page_id)
                            .build(),
                    )
                    .dimensions(
                        Dimension::builder()
                            .name("client_id")
                            .value(&meta.client_id)
                            .build(),
                    )
                    .dimensions(
                        Dimension::builder()
                            .name("deploy_type")
                            .value(meta.deploy_type.as_ref())
                            .build(),
                    )
                    .dimensions(
                        Dimension::builder()
                            .name("total_files")
                            .value(file.total().to_string())
                            .build(),
                    )
                    .build(),
            )
            .send()
            .await
        {
            error!(?err, "Fail to send metric");
        }
    }
}
