use crate::{errors::ProcessFileError, retry::retry, types::DeployMeta};
use bstr::ByteSlice;
use once_cell::sync::Lazy;
use path_macro::path;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Once,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Command,
    task::JoinHandle,
    time::timeout,
};
use tracing::{info, instrument, warn};

pub static WRANGLER_ROOT: &str = "/tmp/wrangler_root";
static WRANGLER_CACHE_DIR: &str = "/tmp/wrangler_root/node_modules";
static CREATE_DIR_ONCE: Once = Once::new();
const WRANGLER_TIMEOUT_SECS: u64 = 60 * 20; // 20 minutes
const WRANGLER_STATIC_TIMEOUT_SECS: u64 = 60 * 60; // 60 minutes

static WRANGLER_PATH: Lazy<PathBuf> = Lazy::new(|| {
    let pwd = std::env::current_dir().expect("Can't find current directory");
    path!(pwd / "node_modules" / ".bin" / "wrangler")
});

pub fn init() {
    CREATE_DIR_ONCE.call_once(|| {
        if let Err(err) = fs::create_dir_all(WRANGLER_CACHE_DIR) {
            sentry::capture_error(&err);
        }
    });
}

#[instrument(err)]
pub async fn spawn(
    meta: &DeployMeta,
    site_root: &Path,
    deploy_path: &Path,
) -> Result<(), ProcessFileError> {
    // HACK: path to trick wrangler and make it place cache in a writable path
    fs::create_dir_all(WRANGLER_CACHE_DIR)?;
    let res = retry(|| async {
        match timeout(
            Duration::from_secs(if meta.is_static() {
                WRANGLER_STATIC_TIMEOUT_SECS
            } else {
                WRANGLER_TIMEOUT_SECS
            }),
            do_spawn(meta, deploy_path, site_root),
        )
        .await
        {
            Ok(Ok(())) => Ok(()),
            Ok(Err(err)) => Err(err),
            Err(elapsed) => Err(ProcessFileError::WranglerTimeout(elapsed)),
        }
    })
    .await
    .map_err(ProcessFileError::from);

    cleanup_wrangler().await;

    res
}

async fn do_spawn(
    meta: &DeployMeta,
    deploy_path: &Path,
    site_root: &Path,
) -> Result<(), ProcessFileError> {
    let args = [
        &*WRANGLER_PATH.as_os_str(),
        "pages".as_ref(),
        "deploy".as_ref(),
        "--project-name".as_ref(),
        meta.page_id.as_ref(),
        "--branch".as_ref(),
        meta.client_id.as_ref(),
        deploy_path.as_os_str(),
    ];

    let wrangler_args = &args[1..];
    info!(args = ?wrangler_args, "run wrangler");
    let mut child = Command::new("node")
        .args(args)
        .current_dir(site_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout_reader = spawn_reader("stdout", child.stdout.take(), |line| {
        tracing::info!("{}", line);
    });
    let stderr_reader = spawn_reader("stderr", child.stderr.take(), |line| {
        tracing::warn!("{}", line);
    });

    let status = child.wait().await?;
    let success = status.success();
    let _ = tokio::join!(stdout_reader, stderr_reader);

    if success {
        Ok(())
    } else {
        Err(ProcessFileError::DeployFail(status.code()))
    }
}

async fn spawn_reader(
    channel: &'static str,
    output: Option<impl AsyncRead + Send + Unpin + 'static>,
    log: fn(&str) -> (),
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let reader = BufReader::new(output.unwrap_or_else(|| {
            panic!("Fail to get {}", channel);
        }));
        let mut lines = reader.lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    log(&line);
                }
                Ok(None) => break,
                Err(err) => {
                    sentry::capture_error(&err);
                    break;
                }
            }
        }
    })
}

/// Clean up the junk files leave by wrangler
/// `/tmp/*.mjs`
/// `/tmp/*.map`
/// `/tmp/tmp-*/`
#[instrument]
async fn cleanup_wrangler() {
    let mut files = match tokio::fs::read_dir("/tmp/").await {
        Ok(files) => files,
        Err(err) => {
            warn!(?err, "Fail to read dir");
            return;
        }
    };

    loop {
        let entry = files.next_entry().await;
        match entry {
            Ok(None) => break,
            Ok(Some(entry)) => {
                let file_name = entry.file_name();
                let name = to_bstr(&file_name);
                if name.ends_with(b".mjs") || name.ends_with(b".map") {
                    if let Err(err) = tokio::fs::remove_file(entry.path()).await {
                        warn!(?err, "Fail to remove file");
                    }
                } else if name.starts_with(b"tmp-")
                    && entry.file_type().await.is_ok_and(|ty| ty.is_dir())
                {
                    if let Err(err) = tokio::fs::remove_dir(entry.path()).await {
                        warn!(?err, "Fail to remove dir");
                    }
                }
            }
            Err(err) => {
                warn!(?err, "Fail to read dir entry");
            }
        }
    }
}

/// Convert OsStr to byte slice
/// As read_dir will return OsStr
fn to_bstr(s: &impl AsRef<OsStr>) -> &[u8] {
    let bs = <[u8] as ByteSlice>::from_os_str(AsRef::<OsStr>::as_ref(s));
    match bs {
        Some(bs) => bs,
        None => {
            // This should never happen on Unix-like system
            unreachable!("filepath is not byte slice")
        }
    }
}
