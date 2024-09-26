use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

use crate::http::CLIENT;

pub async fn submit_sitemap(url: &str) -> reqwest_middleware::Result<()> {
    CLIENT
        .get(format!(
            "https://www.google.com/ping?sitemap={}",
            utf8_percent_encode(
                // Must use the xml path
                &format!("https://{url}/sitemap-index.xml"),
                &NON_ALPHANUMERIC
            )
        ))
        .send()
        .await?;
    Ok(())
}
