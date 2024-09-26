use crate::{
    api::operation::{Operation, ToResponse},
    types::DeployMeta,
};
use graphql_client::QueryBody;
use serde_derive::Deserialize;

#[derive(Debug)]
pub struct GetSite;

impl GetSite {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl Operation for GetSite {
    type Request<'a> = QueryBody<()>;

    fn name(&self) -> &'static str {
        "getSite"
    }

    #[inline]
    fn request<'a>(&'a self, _meta: &'a DeployMeta) -> Self::Request<'a> {
        build_query()
    }
}

impl ToResponse for GetSite {
    type Response = GetSiteResponse;
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone)]
pub struct GetSiteResponseInner {
    customer_site_domain: String,
    customer_site_storipress_url: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetSiteResponse {
    site: GetSiteResponseInner,
}

impl GetSiteResponse {
    pub fn customer_site_storipress_url(&self) -> &str {
        &self.site.customer_site_storipress_url
    }

    pub fn customer_site_domain(&self) -> &str {
        &self.site.customer_site_domain
    }
}

#[inline]
fn build_query() -> QueryBody<()> {
    QueryBody {
        variables: (),
        query: include_str!("./get_site.gql"),
        operation_name: "GetSite",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::operations::test_helper::assert_operation;

    #[tokio::test]
    #[ignore] // default disable as it will request API
    async fn test_operation_work() {
        assert_operation(GetSite::new()).await;
    }
}
