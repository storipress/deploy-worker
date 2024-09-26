use serde_derive::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, strum::Display)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseState {
    Done,
    Aborted,
    Canceled,
    Queued,
    Error,
    Preparing,
    Generating,
    Compressing,
    Uploading,
}
