use crate::types::DeployMeta;
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;

pub trait ToResponse: Debug {
    type Response: DeserializeOwned + Debug;
}

impl<T: ToResponse + Debug> ToResponse for &T {
    type Response = <T as ToResponse>::Response;
}

pub trait Operation: Debug + ToResponse {
    type Request<'a>: Serialize + Debug
    where
        Self: 'a;

    fn name(&self) -> &'static str;

    fn request<'a>(&'a self, meta: &'a DeployMeta) -> Self::Request<'a>;
}

impl<T: Operation + ToResponse + Debug> Operation for &T {
    type Request<'a> = <T as Operation>::Request<'a> where Self: 'a;

    fn name(&self) -> &'static str {
        (*self).name()
    }

    fn request<'a>(&'a self, meta: &'a DeployMeta) -> Self::Request<'a> {
        (*self).request(meta)
    }
}
