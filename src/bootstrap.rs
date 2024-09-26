use crate::constants::SENTRY_DSN;
use sentry::ClientInitGuard;
use std::env;
use tracing::Level;
use tracing_subscriber::{filter::filter_fn, prelude::*, EnvFilter};

pub struct BootstrapGuard(ClientInitGuard, tracing_axiom::Guard);

pub fn init() -> BootstrapGuard {
    let (axiom_layer, axiom_guard) = tracing_axiom::builder()
        .with_service_name("deployer")
        .layer()
        .expect("Fail to init axiom layer");

    let guard = sentry::init((
        SENTRY_DSN,
        sentry::ClientOptions {
            release: sentry::release_name!(),
            traces_sample_rate: 0.0,
            ..sentry::ClientOptions::default()
        },
    ));

    if env::var("CARGO_MANIFEST_DIR").is_ok() {
        tracing_subscriber::registry()
            .with(sentry_tracing::layer())
            .with(tracing_subscriber::fmt::layer().pretty().with_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            ))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(axiom_layer.with_filter(filter_fn(move |meta| {
                meta.level() <= &Level::DEBUG && meta.target().starts_with(env!("CARGO_PKG_NAME"))
            })))
            .with(sentry_tracing::layer())
            .with(tracing_subscriber::fmt::layer().json().with_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                    EnvFilter::new("info,deployer_service=debug,deployer=debug")
                }),
            ))
            .init();
    };

    BootstrapGuard(guard, axiom_guard)
}

#[cfg(test)]
mod tests {
    use tracing::Level;

    #[test]
    fn test_level_filer() {
        assert!(!(Level::TRACE <= Level::DEBUG));
        assert!(Level::DEBUG <= Level::DEBUG);
        assert!(Level::INFO <= Level::DEBUG);
        assert!(Level::WARN <= Level::DEBUG);
        assert!(Level::ERROR <= Level::DEBUG);
    }
}
