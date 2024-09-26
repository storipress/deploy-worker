use serde_json::Error;
use std::{
    error::Error as StdError,
    fmt::{self, Debug, Display},
};
use tokio::{task::JoinError, time::error::Elapsed};

#[derive(Debug, thiserror::Error)]
pub enum ProcessFileError {
    #[error("S3 error")]
    S3Error,
    #[error("Empty meta list")]
    EmptyMeta,
    #[error("Storipress meta not found")]
    NoMeta,
    #[error("Invalid meta {meta}")]
    InvalidMeta {
        meta: String,
        #[source]
        source: Error,
    },
    #[error("Deploy fail {:?}", 0)]
    DeployFail(Option<i32>),

    #[error("wrangler timeout after {}", 0)]
    WranglerTimeout(#[from] Elapsed),

    #[error(transparent)]
    AggregateError(#[from] AggregateError<Box<ProcessFileError>>),

    #[error(transparent)]
    R2Error(#[from] crate::put_directory::Error),

    #[cfg(feature = "intended_fail")]
    #[error("This is a intended fail which is for testing")]
    IntendFail,

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Join error")]
    JoinError(#[source] JoinError),
}

#[derive(Debug)]
pub struct AggregateError<E: StdError + Debug + Send + Sync + 'static> {
    root: AggregateErrorNode<E>,
}

impl<E: StdError + Debug + Send + Sync + 'static> AggregateError<E> {
    pub fn into_vec(self) -> Vec<E> {
        let mut res = vec![];
        let AggregateErrorNode { error, mut next } = self.root;
        res.push(error);
        while let Some(node) = next {
            res.push(node.error);
            next = node.next;
        }
        res
    }
}

impl<E: StdError + Debug + Send + Sync + 'static> IntoIterator for AggregateError<E> {
    type Item = E;
    type IntoIter = <Vec<E> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.into_vec().into_iter()
    }
}

impl<E: StdError + Debug + Send + Sync + 'static> Display for AggregateError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AggregateError")
    }
}

impl<E: StdError + Debug + Send + Sync + 'static> StdError for AggregateError<E> {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.root.error)
    }
}

impl<E: StdError + Debug + Send + Sync + 'static> AggregateError<E> {
    pub fn new(error: E) -> Self {
        Self {
            root: AggregateErrorNode { error, next: None },
        }
    }

    pub fn from_iter<L: IntoIterator<Item = E>>(errors: L) -> Self {
        let mut iter = errors.into_iter();
        let mut root = AggregateErrorNode {
            error: iter.next().expect("Expect at least one error"),
            next: None,
        };
        let mut current = &mut root;
        for error in iter {
            current.next = Some(Box::new(AggregateErrorNode { error, next: None }));
            current = current.next.as_mut().unwrap();
        }
        Self { root }
    }
}

impl<E: StdError + Debug + Send + Sync + 'static> From<Vec<E>> for AggregateError<E> {
    fn from(errors: Vec<E>) -> Self {
        Self::from_iter(errors)
    }
}

#[derive(Debug)]
struct AggregateErrorNode<E: StdError + Debug + Send + Sync + 'static> {
    error: E,
    next: Option<Box<AggregateErrorNode<E>>>,
}

impl<E: StdError + Debug + Send + Sync + 'static> Display for AggregateErrorNode<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.error, f)
    }
}

impl<E: StdError + Debug + Send + Sync + 'static> StdError for AggregateErrorNode<E> {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.next.as_ref().map(|err| err as _)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;

    #[test]
    fn test_aggregate_error() {
        let err = AggregateError::from(vec![
            Box::new(ProcessFileError::S3Error),
            Box::new(ProcessFileError::NoMeta),
        ]);

        assert_debug_snapshot!(err, @r###"
        AggregateError {
            root: AggregateErrorNode {
                error: S3Error,
                next: Some(
                    AggregateErrorNode {
                        error: NoMeta,
                        next: None,
                    },
                ),
            },
        }
        "###);
    }
}
