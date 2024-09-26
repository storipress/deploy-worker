use once_cell::sync::Lazy;
use reqwest::{Client, ClientBuilder as RClientBuilder};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use reqwest_tracing::TracingMiddleware;
use std::convert::identity;
use tap::Pipe;

static APP_USER_AGENT: &str = concat!("storipress-deployer/", env!("CARGO_PKG_VERSION"),);

pub(crate) static CLIENT: Lazy<ClientWithMiddleware> = Lazy::new(|| build_client(identity));

pub(crate) fn build_client(
    builder: impl FnOnce(RClientBuilder) -> RClientBuilder,
) -> ClientWithMiddleware {
    let client = Client::builder()
        .user_agent(APP_USER_AGENT)
        .pipe(builder)
        .build()
        .expect("Fail to init http client");

    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    let client = ClientBuilder::new(client)
        // Trace HTTP requests. See the tracing crate to make use of these traces.
        .with(TracingMiddleware::default())
        // Retry failed requests.
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build();
    client
}
