use super::types::ReleaseState;
use crate::{
    api::operation::{Operation, ToResponse},
    types::DeployMeta,
};
use graphql_client::QueryBody;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug)]
pub struct UpdateRelease {
    state: ReleaseState,
}

impl UpdateRelease {
    pub fn new(state: ReleaseState) -> Self {
        Self { state }
    }
}

impl Operation for UpdateRelease {
    type Request<'a> = QueryBody<UpdateReleaseVariables<'a>>;

    fn name(&self) -> &'static str {
        "updateRelease"
    }

    #[inline]
    fn request<'a>(&'a self, meta: &'a DeployMeta) -> Self::Request<'a> {
        build_query(UpdateReleaseVariables {
            input: UpdateReleaseInput {
                id: &meta.release_id,
                state: self.state,
            },
        })
    }
}

impl ToResponse for UpdateRelease {
    type Response = UpdateReleaseResponse;
}

#[derive(Serialize, Debug, Clone)]
pub struct UpdateReleaseInput<'a> {
    pub id: &'a str,
    pub state: ReleaseState,
}

#[derive(Serialize, Debug)]
pub struct UpdateReleaseVariables<'a> {
    pub input: UpdateReleaseInput<'a>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone)]
pub struct UpdateReleaseResponseInner {
    id: String,
    state: ReleaseState,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateReleaseResponse {
    update_release: UpdateReleaseResponseInner,
}

#[inline]
fn build_query<'a>(variables: UpdateReleaseVariables<'_>) -> QueryBody<UpdateReleaseVariables<'_>> {
    QueryBody {
        variables,
        query: include_str!("./update_release.gql"),
        operation_name: "UpdateRelease",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::operations::test_helper::assert_operation;

    #[tokio::test]
    #[ignore] // default disable as it will request API
    async fn test_operation_work() {
        assert_operation(UpdateRelease::new(ReleaseState::Done)).await;
    }
}
