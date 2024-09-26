use aws_sdk_sqs::Client;
use futures::FutureExt;
use std::{future::Future, mem, pin::Pin, time::Duration};
use tokio::{task, time};

const INITIAL_TIMEOUT: i32 = 240;
const PREPARE_TIME: u64 = 10;

pub struct HeartBeat<'a> {
    client: &'a Client,
    queue_url: &'a str,
    message_handle: &'a str,
}

impl<'a> HeartBeat<'a> {
    pub fn new(client: &'a Client, queue_url: &'a str, message_handle: &'a str) -> Self {
        Self {
            client,
            queue_url,
            message_handle,
        }
    }

    pub async fn run<F, FN>(&'a self, f: FN)
    where
        F: Future<Output = ()> + Send,
        FN: (Fn() -> F) + Send + Sync + 'a,
    {
        let mut timeout_extending_count = 0;

        let mut current_timeout = INITIAL_TIMEOUT;

        // Safety: we are sure that we will join the task
        let worker = unsafe {
            mem::transmute::<_, Pin<Box<dyn Future<Output = ()> + Send + 'static>>>(
                async move {
                    f().await;
                }
                .boxed(),
            )
        };
        // move task to background
        let mut join_handle = task::spawn(worker);

        loop {
            tokio::select! {
                biased;
                _ = time::sleep(Duration::from_secs((current_timeout as u64) - PREPARE_TIME)) => {
                    // This branch is biased so we can ensure to run extend timeout first

                    timeout_extending_count += 1;
                    if timeout_extending_count > 10 {
                        sentry::capture_message(
                            "Timeout extending too many time",
                            sentry::Level::Error,
                        );
                        break;
                    }

                    sentry::capture_message(
                        &format!(
                            "Extending timeout for {} time",
                            timeout_extending_count
                        ),
                        sentry::Level::Info,
                    );

                    self.extend_timeout().await;
                    current_timeout = 60;
                }
                _ = &mut join_handle => break,
            }
        }
    }

    async fn extend_timeout(&self) {
        let res = self
            .client
            .change_message_visibility()
            .queue_url(self.queue_url)
            .receipt_handle(self.message_handle)
            .visibility_timeout(60)
            .send()
            .await;

        if let Err(err) = res {
            sentry::capture_error(&err);
        }
    }
}
