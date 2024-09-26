use std::{future::Future, time::Duration};
use tokio::time;
use tracing::warn;

use crate::errors::AggregateError;

const RETRY_LIMIT: i32 = 1;
const DELAY_SECS_BETWEEN_RETRY: u64 = 2;

pub async fn retry<Func, Return, ErrType>(f: Func) -> Result<(), AggregateError<Box<ErrType>>>
where
    Func: FnMut() -> Return,
    Return: Future<Output = Result<(), ErrType>>,
    ErrType: std::error::Error + Send + Sync + 'static,
{
    retry_with_limit(RETRY_LIMIT, f).await
}

async fn retry_with_limit<Func, Return, ErrType>(
    limit: i32,
    mut f: Func,
) -> Result<(), AggregateError<Box<ErrType>>>
where
    Func: FnMut() -> Return,
    Return: Future<Output = Result<(), ErrType>>,
    ErrType: std::error::Error + Send + Sync + 'static,
{
    let mut retry = 0;
    let mut errors = vec![];
    loop {
        match f().await {
            Ok(_) => return Ok(()),
            Err(err) => {
                warn!(?err, "Retry deploy: {}", retry);
                sentry::capture_message("Retry deploy", sentry::Level::Warning);
                errors.push(Box::new(err));
                if retry < limit {
                    retry += 1;
                    time::sleep(Duration::from_secs(DELAY_SECS_BETWEEN_RETRY)).await;
                } else {
                    return Err(AggregateError::from(errors));
                }
            }
        }
    }
}
