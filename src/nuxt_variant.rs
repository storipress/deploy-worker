use std::{io, path::Path};

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum NuxtVariant {
    Function,
    Worker,
}

impl NuxtVariant {
    pub fn check_path(path: &Path) -> io::Result<Self> {
        match path.join("dist/nitro.json").metadata() {
            // when nitro.json is a file, we assume it's a worker
            Ok(meta) if meta.is_file() => return Ok(NuxtVariant::Worker),
            // when nitro.json is not a file, because it still have `dist` folder, we assume it's a worker
            Ok(meta) => {
                sentry::capture_message(
                    &format!(
                        "unexpected file type for nitro.json: {:?}",
                        meta.file_type()
                    ),
                    sentry::Level::Warning,
                );
                Ok(NuxtVariant::Worker)
            }
            // when nitro.json is not found, we assume it's a function
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(NuxtVariant::Function),
            // unexpected error
            Err(err) => Err(err),
        }
    }

    pub fn as_public_path(self) -> &'static str {
        match self {
            NuxtVariant::Worker => "dist",
            NuxtVariant::Function => ".output/public",
        }
    }

    pub fn guess_public_path(path: &Path) -> &'static str {
        match Self::check_path(path) {
            Ok(variant) => variant.as_public_path(),
            Err(err) => {
                sentry::capture_error(&err);
                "dist"
            }
        }
    }
}
