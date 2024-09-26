use crate::{
    api::{get_site, update_release, Client, ReleaseState},
    check_version::wait_version_match,
    clean_files::clean_unused_files,
    errors::ProcessFileError,
    metric,
    nuxt_variant::NuxtVariant,
    put_directory::put_directory,
    sitemap::submit_sitemap,
    types::{DeployMeta, DeployType, FileSummary},
    verify_site::verify_site,
    wrangler::{self, WRANGLER_ROOT},
};
use aws_config::BehaviorVersion;
use aws_credential_types::{provider::SharedCredentialsProvider, Credentials};
use aws_lambda_events::s3::{S3Bucket, S3Entity, S3Event, S3EventRecord, S3Object};
use aws_sdk_s3::{error::SdkError, operation::get_object::GetObjectError};
use aws_types::region::Region;
use once_cell::sync::Lazy;
use percent_encoding::percent_decode;
use scopeguard::ScopeGuard;
use serde_derive::Serialize;
use std::{convert::Infallible, env, path::Path};
use tap::prelude::*;
use tempfile::tempdir_in;
use tokio::{io::AsyncRead, runtime::Handle};
use tokio_util::io::SyncIoBridge;
use tracing::{debug, debug_span, error, info, info_span, instrument, warn, Instrument};

#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub processed: Vec<String>,
}

#[derive(Debug, Serialize, thiserror::Error)]
#[error("Fail to deploy")]
pub struct FailureResponse {
    pub success: Vec<String>,
    pub failed: Vec<String>,
}

pub type Response = Result<SuccessResponse, FailureResponse>;

static R2_CREDENTIALS: Lazy<SharedCredentialsProvider> = Lazy::new(|| {
    SharedCredentialsProvider::new(Credentials::new(
        env::var("R2_ACCESS_KEY").expect("R2_ACCESS_KEY is missing"),
        env::var("R2_SECRET_KEY").expect("R2_SECRET_KEY is missing"),
        None,
        None,
        "r2",
    ))
});

#[instrument(ret, err)]
pub async fn handle_s3_event(payload: S3Event) -> Response {
    info!(?payload, "handling a request...");

    let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
    let s3_client = aws_sdk_s3::Client::new(&config);
    let cw_client = aws_sdk_cloudwatch::Client::new(&config);
    let mut processed = Vec::new();
    let mut failed = Vec::new();

    info!("total records: {}", payload.records.len());
    let records = payload
        .records
        .into_iter()
        .filter_map(|mut record| {
            let S3EventRecord {
                ref event_name,
                s3:
                    S3Entity {
                        bucket:
                            S3Bucket {
                                name: ref mut bucket,
                                ..
                            },
                        object: S3Object { ref mut key, .. },
                        ..
                    },
                ..
            } = record;
            if event_name
                .as_deref()
                .map(|name| name.contains("Copy"))
                .unwrap_or(false)
            {
                sentry::capture_message(
                    "Detect old CDN compatible feature is enabling in generator",
                    sentry::Level::Warning,
                );
            }
            let (bucket, key) = match (bucket.take(), key.take()) {
                (Some(bucket), Some(key)) => (bucket, key),
                (bucket, key) => {
                    warn!(?record, bucket, key, "no bucket or key");
                    return None;
                }
            };
            Some((bucket, key))
        })
        .collect::<Vec<_>>();

    for (bucket, key) in records {
        let handling_span = debug_span!("handling request", bucket, key);
        let _handling_guard = handling_span.enter();

        let key = match percent_decode(key.as_bytes()).decode_utf8() {
            Ok(key) => key.into_owned(),
            Err(err) => {
                error!(key, "Fail to decode key");
                sentry::capture_error(&err);
                failed.push(key);
                continue;
            }
        };

        // TODO: retry if error
        // TODO: parallel handling if possible
        if let Err(err) = process_file(&s3_client, &cw_client, &bucket, &key).await {
            error!(?err, "Error when process {bucket}/{key}");
            sentry::capture_error(&err);
            failed.push(key);
            continue;
        }

        if let Err(err) = s3_client
            .delete_object()
            .bucket(&bucket)
            .key(&key)
            .send()
            .await
        {
            error!(?err, "Fail to cleanup {bucket}/{key}");
            sentry::capture_error(&err);
        }

        processed.push(key);
    }

    if failed.is_empty() {
        Ok(SuccessResponse { processed })
    } else {
        Err(FailureResponse {
            success: processed,
            failed,
        })
    }
}

#[instrument(err, skip(s3_client, cw_client))]
pub async fn process_file(
    s3_client: &aws_sdk_s3::Client,
    cw_client: &aws_sdk_cloudwatch::Client,
    bucket: &str,
    key: &str,
) -> Result<(), ProcessFileError> {
    wrangler::init();
    let metric_guard = metric::start(cw_client);

    let Some((mut meta, body_stream)) = get_file(s3_client, bucket, key).await? else {
        // file already processed
        return Ok(());
    };

    meta.derive_deploy_type_from_source();

    let client = Client::new(meta);

    let executor = Handle::current();
    let client = scopeguard::guard(client, |client| {
        executor.spawn(async move {
            update_release(&client, ReleaseState::Error).await;
        });
    });

    let meta = &client.meta;

    let span = info_span!(
        "process_file_span",
        bucket = bucket,
        key = key,
        client_id = meta.client_id,
        release_id = meta.release_id,
        source = meta.source,
        deploy_type = ?meta.deploy_type,
    );

    let summary = do_process_file(&client, bucket, key, body_stream)
        .instrument(span)
        .await?;

    let client = ScopeGuard::into_inner(client);

    metric_guard.stop(&client.meta, &summary).await;

    tokio::spawn(async move {
        verify_site(client).await;
    });
    Ok(())
}

#[instrument(err, skip(body_stream))]
async fn do_process_file(
    api_client: &Client,
    bucket: &str,
    key: &str,
    body_stream: impl AsyncRead + Unpin + Send,
) -> Result<FileSummary, ProcessFileError> {
    let meta = &api_client.meta;
    info!(
        ?meta,
        "start processing {bucket}/{key} from {source:?} with {deploy_type:?}",
        source = meta.source,
        deploy_type = meta.deploy_type,
    );

    #[cfg(feature = "intended_fail")]
    if meta.__storipress_deployer_force_error {
        return Err(ProcessFileError::IntendFail);
    }

    update_release(api_client, ReleaseState::Uploading).await;
    let dir = tempdir_in(WRANGLER_ROOT)?;
    let tmp_path = dir.path();
    info!("extract to {}", tmp_path.display());
    extract_to(body_stream, tmp_path).await?;
    let summary = clean_unused_files(tmp_path);

    let (site_root, deploy_path) = match (meta.deploy_type, meta.output_path.as_deref()) {
        (DeployType::CloudflareFunction, None) => (
            tmp_path,
            NuxtVariant::guess_public_path(tmp_path).pipe(AsRef::<Path>::as_ref),
        ),
        (DeployType::Static, None) => (tmp_path, tmp_path),
        (_, Some(output_path)) => (tmp_path, output_path.pipe(AsRef::<Path>::as_ref)),
    };

    debug!(site_root = %site_root.display(), deploy_path = %deploy_path.display(), "detect root");

    if !meta.is_static() {
        info!("put to r2");
        let r2_client = create_r2_client();
        let local_path = tmp_path.join(deploy_path).join("_nuxt");

        put_directory(
            &r2_client,
            "storipress",
            &format!("{}/_nuxt", api_client.meta.client_id),
            &local_path,
        )
        .await?;

        info!("r2 put success");
    } else {
        info!("skip put to r2");
    }

    debug!(?site_root, ?deploy_path, "detect root");

    wrangler::spawn(&meta, site_root, deploy_path).await?;

    update_release(api_client, ReleaseState::Done).await;

    info!("Deploy success success");

    if let Err(err) = do_submit_sitemap(api_client).await {
        error!(?err, "Fail to submit sitemap");
    }

    Ok(summary)
}

#[instrument]
async fn do_submit_sitemap(client: &Client) -> anyhow::Result<()> {
    let Some(site) = get_site(client).await? else {
        return Ok(());
    };

    submit_sitemap(site.customer_site_domain()).await?;
    Ok(())
}

#[instrument(err, skip(body_stream))]
async fn extract_to(
    body_stream: impl AsyncRead + Unpin + Send,
    tmp_path: &Path,
) -> Result<(), ProcessFileError> {
    let archive_file = SyncIoBridge::new(body_stream);
    let (res, outputs) = async_scoped::TokioScope::scope_and_block(move |s| {
        s.spawn_blocking(move || {
            let archive_file = brotli::Decompressor::new(archive_file, 4096);
            let mut archive_file = tar::Archive::new(archive_file);
            archive_file.unpack(tmp_path)
        });

        Ok::<_, Infallible>(())
    });

    res.expect("Impossible to fail with async_scoped");

    match outputs.into_iter().next() {
        Some(Ok(Ok(()))) => Ok(()),
        Some(Ok(Err(err))) => Err(ProcessFileError::Io(err)),
        Some(Err(err)) => Err(ProcessFileError::JoinError(err)),
        None => unreachable!("must have a least one item"),
    }
}

#[instrument(err, skip(s3_client))]
async fn get_file(
    s3_client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
) -> Result<Option<(DeployMeta, impl AsyncRead)>, ProcessFileError> {
    let object = match s3_client.get_object().bucket(bucket).key(key).send().await {
        Ok(object) => object,
        Err(err) => match err {
            SdkError::ServiceError(err) if matches!(err.err(), GetObjectError::NoSuchKey(_)) => {
                // file already processed by another worker
                return Ok(None);
            }
            err => {
                error!(?err, "Fail to get object {bucket}/{key}");
                return Err(ProcessFileError::S3Error);
            }
        },
    };
    let meta = object
        .metadata()
        .ok_or(ProcessFileError::EmptyMeta)
        .and_then(|meta_map| {
            info!(?meta_map, "meta list");
            meta_map.get("sp-deploy").ok_or(ProcessFileError::NoMeta)
        })
        .and_then(|value| {
            serde_json::from_str(value).map_err(|source| ProcessFileError::InvalidMeta {
                source,
                meta: value.clone(),
            })
        })?;
    Ok(Some((meta, object.body.into_async_read())))
}

fn create_r2_client() -> aws_sdk_s3::Client {
    let r2_client_config = aws_sdk_s3::Config::builder()
        .endpoint_url("")
        .behavior_version(BehaviorVersion::latest())
        .credentials_provider(R2_CREDENTIALS.clone())
        .region(Region::new("auto"))
        .build();
    let r2_client = aws_sdk_s3::Client::from_conf(r2_client_config);
    r2_client
}
