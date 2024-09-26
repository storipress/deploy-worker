use aws_sdk_s3::Client;
use aws_smithy_types::byte_stream::ByteStream;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use jwalk::WalkDir;
use md5::{Digest, Md5};
use std::{fs, path::Path};
use tokio::{fs::File, io::AsyncReadExt};
use tracing::{debug, error, info, instrument};

use crate::{errors::AggregateError, retry::retry};

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub enum Error {
    Io(#[from] std::io::Error),
    ReadDir(#[from] jwalk::Error),
    StripPrefix(#[from] std::path::StripPrefixError),
    AwsError(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[instrument(err, skip(client, local_path), fields(local_path = %local_path.as_ref().display()))]
pub async fn put_directory(
    client: &Client,
    bucket: &str,
    key_prefix: &str,
    local_path: impl AsRef<Path>,
) -> Result<(), Error> {
    let local_path = local_path.as_ref();
    info!("start put directory");
    let files = fs::read_dir(local_path)
        .map_err(Error::from)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(Error::from)?;
    debug!(local_path = %local_path.display(), ?files, "scan directory");
    for entry in WalkDir::new(local_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let relative_path = path
            .strip_prefix(local_path)
            .map_err(Error::from)?
            .display();
        let key = format!("{}/{}", key_prefix, relative_path);

        let full_path = local_path.join(&path);

        debug!(path = %path.display(), key, relative_path = %relative_path, full_path = %full_path.display(), "process file");

        if let Err(err) = put_object(client, &full_path, bucket, &key).await {
            for err in err {
                error!(?err, "fail to put object");
                sentry::capture_error(&err);
            }
        }
    }
    Ok(())
}

#[instrument(err, skip(client))]
async fn put_object(
    client: &Client,
    full_path: &Path,
    bucket: &str,
    key: &str,
) -> Result<(), AggregateError<Box<Error>>> {
    retry(|| async {
        let stream = ByteStream::from_path(&full_path).await.map_err(|err| {
            Error::from(Box::new(err) as Box<dyn std::error::Error + Send + Sync>)
        })?;

        let guess_mime = mime_guess::from_path(&full_path).first();
        let content_type = guess_mime
            .as_ref()
            .map_or("application/octet-stream", |mime| mime.as_ref());

        let content_md5 = calculate_md5(&full_path).await?;

        debug!(content_md5, content_type, key, "put_object");

        client
            .put_object()
            .bucket(bucket)
            .key(key)
            .content_type(content_type)
            .content_md5(content_md5)
            .body(stream)
            .send()
            .await
            .map_err(
                |err| Error::from(Box::new(err) as Box<dyn std::error::Error + Send + Sync>),
            )?;
        Ok(())
    })
    .await
}

#[instrument(err, skip(path))]
async fn calculate_md5(path: &Path) -> Result<String, Error> {
    let mut file = File::open(path).await.map_err(Error::from)?;
    let mut hasher = Md5::new();
    let mut buffer = [0; 1024];

    loop {
        let bytes_read = file.read(&mut buffer).await.map_err(Error::from)?;
        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    let content_md5 = STANDARD.encode(&hasher.finalize());
    Ok(content_md5)
}
