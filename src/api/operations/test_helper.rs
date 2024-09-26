#![cfg(test)]
use crate::{
    api::{client::Client, operation::Operation},
    types::DeployMeta,
};
use std::env;

pub async fn assert_operation<'a>(op: impl Operation) {
    let (client_id, release_id, token) = init_env();
    let meta = DeployMeta {
        client_id,
        release_id,
        token: Some(token),
        ..Default::default()
    };

    let client = Client::new(meta);
    let res = client.send(&op).await;
    assert!(res.is_ok(), "Fail to send {}: {:?}", op.name(), res);
}

fn init_env() -> (String, String, String) {
    dotenvy::dotenv().ok();
    let client_id = env::var("TEST_CLIENT_ID").expect("TEST_CLIENT_ID not set");
    let release_id = env::var("TEST_RELEASE_ID").expect("TEST_RELEASE_ID not set");
    let token = env::var("TEST_TOKEN").expect("TEST_TOKEN not set");

    (client_id, release_id, token)
}
