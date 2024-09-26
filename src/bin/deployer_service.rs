use aws_config::BehaviorVersion;
use aws_lambda_events::s3::{S3Event, S3EventRecord};
use aws_sdk_sqs::{types::Message, Client, Error};
use deployer::{
    bootstrap, health_check::HealthCheck, heartbeat::HeartBeat, s3_handler, test_event::TestEvent,
};
use std::{env, fmt, time::Duration};
use tokio::{
    select,
    signal::unix::{signal, SignalKind},
    sync::oneshot,
    task,
    time::{interval, MissedTickBehavior},
};
use tracing::{error, info, instrument};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    info!("starting standalone deployer service");
    let _guard = bootstrap::init();
    let (request_stop, mut stop_receiver) = oneshot::channel();
    task::spawn(async {
        let mut interrupt = signal(SignalKind::interrupt()).expect("Fail to listen ctrl+c");
        let mut terminate = signal(SignalKind::terminate()).expect("Fail to listen terminate");
        tokio::select! {
            _ = interrupt.recv() => {
                info!("receive ctrl+c, graceful shutdown")
            },
            _ = terminate.recv() => {
                info!("receive terminate, graceful shutdown")
            }
        }
        request_stop.send(()).expect("Fail to send request stop");
    });

    let mut timer = interval(Duration::from_secs(30));
    let shared_config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let sqs_client = sqs_client(&shared_config);

    timer.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let queue_url = env::var("AWS_QUEUE_URL").expect("No AWS_QUEUE_URL");
    loop {
        select! {
            _ = timer.tick() => (),
            _ = &mut stop_receiver => {
                break;
            }
        }
        receive(&sqs_client, &queue_url)
            .await
            .expect("receive error");
    }

    Ok(())
}

#[derive(Clone)]
struct S3EventRecordFile<'a>(&'a S3EventRecord);

impl<'a> fmt::Debug for S3EventRecordFile<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3EventRecordFile")
            .field("bucket", &self.0.s3.bucket.name)
            .field("key", &self.0.s3.object.key)
            .finish()
    }
}

#[derive(Clone)]
struct S3EventFiles<'a>(&'a S3Event);

impl<'a> fmt::Debug for S3EventFiles<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let records = self
            .0
            .records
            .iter()
            .map(S3EventRecordFile)
            .collect::<Vec<_>>();

        f.debug_struct("S3EventFiles")
            .field("records", &records)
            .finish()
    }
}

#[instrument(skip(client))]
async fn receive(client: &Client, queue_url: &str) -> Result<(), Error> {
    let guard = HealthCheck::start().await;
    let rcv_message_output = client.receive_message().queue_url(queue_url).send().await?;

    let messages = rcv_message_output.messages();

    if messages.is_empty() {
        info!("no message");
    }

    for message in messages {
        if let Some(body) = message.body() {
            let heartbeat = HeartBeat::new(client, queue_url, message.receipt_handle().unwrap());
            heartbeat
                .run(|| async {
                    match serde_json::from_str::<S3Event>(body) {
                        Ok(event) => {
                            let event_files = S3EventFiles(&event);
                            let handle = message.receipt_handle();
                            info!(?event_files, ?handle, "receive s3 event");

                            let res = s3_handler::handle_s3_event(event).await;

                            // only clean the message when success
                            if res.is_ok() {
                                // TODO: consider batch clean up messages
                                info!(?handle, "delete message");
                                delete_message(client, queue_url, message).await;
                            }
                        }

                        Err(err) => match TestEvent::from_str(body) {
                            Ok(event) if event.is_storipress_bucket() => {
                                info!("receive test event");
                                delete_message(client, queue_url, message).await;
                            }
                            Ok(event) => error!(?event, body, "Unknown event"),
                            Err(_) => error!(?err, body, "Fail to parse message"),
                        },
                    }
                })
                .await;
        }
    }

    guard.finish().await;

    Ok(())
}

#[instrument(skip(message))]
async fn delete_message(client: &Client, queue_url: &str, message: &Message) {
    if let Some(handle) = message.receipt_handle() {
        if let Err(err) = client
            .delete_message()
            .queue_url(queue_url)
            .receipt_handle(handle)
            .send()
            .await
        {
            sentry::capture_error(&err);
        }
    }
}

#[instrument]
fn use_localstack() -> bool {
    std::env::var("LOCALSTACK").unwrap_or_default() == "true"
}

#[instrument]
fn sqs_client(conf: &aws_types::SdkConfig) -> aws_sdk_sqs::Client {
    let mut sqs_config_builder = aws_sdk_sqs::config::Builder::from(conf);
    if use_localstack() {
        sqs_config_builder = sqs_config_builder.endpoint_url("http://localhost:4566/")
    }
    aws_sdk_sqs::Client::from_conf(sqs_config_builder.build())
}
