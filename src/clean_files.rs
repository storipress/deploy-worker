use crate::types::FileSummary;
use jwalk::DirEntry;
use std::{ffi::OsStr, fs, path::Path};
use tracing::{debug, error, info, instrument, warn};

pub trait DirEntryLike {
    fn file_name(&self) -> &OsStr;
    fn metadata(&self) -> std::io::Result<std::fs::Metadata>;
}

impl<C: jwalk::ClientState> DirEntryLike for DirEntry<C> {
    fn file_name(&self) -> &OsStr {
        self.file_name()
    }

    fn metadata(&self) -> std::io::Result<std::fs::Metadata> {
        self.metadata().map_err(|err| {
            err.into_io_error().unwrap_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Other, anyhow::anyhow!("unknown error"))
            })
        })
    }
}

fn is_over_25mb(path: &dyn DirEntryLike) -> bool {
    path.metadata()
        .map_or(false, |meta| meta.len() > 25 * 1024 * 1024)
}

fn is_path_ends_with(path: &dyn DirEntryLike, suffix: &str) -> bool {
    path.file_name()
        .to_str()
        .map_or(false, |file_name| file_name.ends_with(suffix))
}

pub(crate) trait CleanRule: Send + Sync {
    fn is_match(&self, entry: &dyn DirEntryLike) -> bool;
}

struct GzCleanRule;

impl CleanRule for GzCleanRule {
    fn is_match(&self, path: &dyn DirEntryLike) -> bool {
        is_path_ends_with(path, ".gz")
    }
}

struct LargeSourceMapRule;

impl CleanRule for LargeSourceMapRule {
    // check filename end with .map + file size > 25mb
    fn is_match(&self, path: &dyn DirEntryLike) -> bool {
        is_path_ends_with(path, ".map") && is_over_25mb(path)
    }
}

struct LargeAtomRule;

impl CleanRule for LargeAtomRule {
    // check filename end with .map + file size > 25mb
    fn is_match(&self, path: &dyn DirEntryLike) -> bool {
        is_path_ends_with(path, "atom.xml") && is_over_25mb(path)
    }
}

static CLEAN_RULES: &[&dyn CleanRule] = &[&GzCleanRule, &LargeSourceMapRule, &LargeAtomRule];

#[instrument]
pub(crate) fn clean_unused_files(root: &Path) -> FileSummary {
    info!("start clean unused file");
    let mut removed_success = 0;
    let mut removed_fail = 0;
    let mut ignored = 0;

    let _ =
        async_scoped::TokioScope::scope_and_block(|scope| {
            scope.spawn_blocking(|| {
            let walker = jwalk::WalkDir::new(root);

            let removed_success = &mut removed_success;
            let removed_fail = &mut removed_fail;
            let ignored = &mut ignored;

            for entry in walker {
                match entry {
                    Ok(entry) if CLEAN_RULES.iter().copied().any(|rule| rule.is_match(&entry)) => {
                        match fs::remove_file(entry.path()) {
                            Ok(_) => {
                                debug!(path = %entry.path().display(), "remove file");
                                *removed_success += 1;
                            }
                            Err(err) => {
                                error!(?err, path = %entry.path().display(), "fail to remove file");
                                *removed_fail += 1;
                            }
                        }
                    }
                    Ok(entry) => {
                        debug!(path = %entry.path().display(), "ignore file");
                        *ignored += 1;
                    }
                    Err(err) => {
                        error!(?err, "fail to read file entry");
                    }
                }
            }

            info!(
                total = *removed_fail + *removed_success + *ignored,
                removed_success, ignored, removed_fail, "summary clean result"
            );
        });
        });
    FileSummary {
        ignored,
        removed_fail,
        removed_success,
    }
}
