use super::operation::{Operation, ToResponse};
#[cfg(test)]
use crate::http::build_client;
#[cfg(not(test))]
use crate::http::CLIENT;
use crate::types::DeployMeta;
use anyhow::{Context, Error};
use graphql_client::Response;
use once_cell::sync::OnceCell;
use reqwest_tracing::OtelName;
use serde_json;
#[cfg(test)]
use std::convert::identity;
use tracing::{instrument, warn};

#[derive(Debug)]
pub struct Client {
    pub meta: DeployMeta,
    api_host: OnceCell<String>,
}

impl Client {
    #[instrument]
    pub fn new(meta: DeployMeta) -> Self {
        Self {
            meta,
            api_host: OnceCell::new(),
        }
    }

    #[instrument]
    pub async fn send<Op: Operation>(
        &self,
        op: Op,
    ) -> anyhow::Result<Option<<Op as ToResponse>::Response>> {
        let api_host = self.api_host()?;
        let token = self.token();

        #[cfg(not(test))]
        let client = &*CLIENT;

        // Must rebuild the client as client will be bound to runtime + test will recreate runtime for each test
        #[cfg(test)]
        let client = &build_client(identity);

        let res = client
            .post(api_host)
            .bearer_auth(token)
            .with_extension(OtelName(op.name().into()))
            .json(&op.request(&self.meta))
            .send()
            .await
            .with_context(|| format!("Fail to send {} API", op.name()))?;
        let response = res
            .text()
            .await
            .with_context(|| format!("Fail to read {} API response", op.name()))?;

        let response: Response<Op::Response> = match serde_json::from_str(&response) {
            Ok(res) => res,
            Err(err) => {
                // log response for debugging
                warn!(
                    ?err,
                    response,
                    ?op,
                    "Fail to parse {} API response",
                    op.name()
                );
                return Err(
                    Error::new(err).context(format!("Fail to parse {} API response", op.name()))
                );
            }
        };
        if let Some(errors) = response.errors {
            return Err(anyhow::anyhow!(
                "Error response from {}: {:?}",
                op.name(),
                errors
            ));
        }

        Ok(response.data)
    }

    #[inline]
    fn api_host(&self) -> anyhow::Result<&String> {
        self.api_host.get_or_try_init(|| self.meta.api_host())
    }

    #[inline]
    fn token(&self) -> &str {
        self.meta.token()
    }
}
