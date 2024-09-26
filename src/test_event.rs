use serde_derive::Deserialize;

/// Test event when config S3 notifaction
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TestEvent {
    service: String,
    event: String,
    bucket: Option<String>,
}

impl TestEvent {
    pub fn from_str(input: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<Self>(input)
    }

    pub fn is_storipress_bucket(&self) -> bool {
        self.service == "Amazon S3"
            && self.event == "s3:TestEvent"
            && self.bucket.as_deref() == Some("storipress")
    }
}
