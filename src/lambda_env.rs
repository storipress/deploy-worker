use once_cell::sync::Lazy;
use sentry::protocol::Context;
use serde_json::Value;
use std::env;

pub static LAMBDA_ENV: Lazy<Option<LambdaEnv>> = Lazy::new(|| LambdaEnv::from_env());

// Modify from https://github.com/awslabs/aws-lambda-rust-runtime/blob/master/lambda-runtime/src/lib.rs#L33
// We don't want it to panic when the environment variable is not set.
/// Configuration derived from environment variables.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LambdaEnv {
    /// The name of the function.
    pub function_name: Option<String>,
    /// The amount of memory available to the function in MB.
    pub memory: Option<i32>,
    /// The version of the function being executed.
    pub version: Option<String>,
    /// The name of the Amazon CloudWatch Logs stream for the function.
    pub log_stream: Option<String>,
    /// The name of the Amazon CloudWatch Logs group for the function.
    pub log_group: Option<String>,
}

impl LambdaEnv {
    /// Attempts to read configuration from environment variables.
    pub fn from_env() -> Option<Self> {
        let conf = LambdaEnv {
            function_name: env::var("AWS_LAMBDA_FUNCTION_NAME").ok(),
            memory: env::var("AWS_LAMBDA_FUNCTION_MEMORY_SIZE")
                .ok()
                .and_then(|s| s.parse::<i32>().ok()),
            version: env::var("AWS_LAMBDA_FUNCTION_VERSION").ok(),
            log_stream: env::var("AWS_LAMBDA_LOG_STREAM_NAME").ok(),
            log_group: env::var("AWS_LAMBDA_LOG_GROUP_NAME").ok(),
        };
        if conf.function_name.is_none() {
            return None;
        }
        Some(conf)
    }
}

pub fn sentry_lambda() {
    sentry::configure_scope(|scope| {
        if let Some(env) = LAMBDA_ENV.as_ref() {
            let mut map = std::collections::BTreeMap::<String, _>::new();
            map.insert(
                "function_name".into(),
                env.function_name
                    .clone()
                    .map(|v| Value::from(v))
                    .unwrap_or(Value::Null),
            );
            map.insert(
                "memory".into(),
                env.memory.map(|v| Value::from(v)).unwrap_or(Value::Null),
            );
            map.insert(
                "version".into(),
                env.version
                    .clone()
                    .map(|v| Value::from(v))
                    .unwrap_or(Value::Null),
            );
            map.insert(
                "log_stream".into(),
                env.log_stream
                    .clone()
                    .map(|v| Value::from(v))
                    .unwrap_or(Value::Null),
            );
            map.insert(
                "log_group".into(),
                env.log_group
                    .clone()
                    .map(|v| Value::from(v))
                    .unwrap_or(Value::Null),
            );

            scope.set_context("lambda_env".into(), Context::Other(map));
        }
    });
}
