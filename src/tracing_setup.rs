use crate::utils::built_info;
use anyhow::{Result, anyhow};
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::{WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::trace::Tracer;
use tracing_subscriber::{EnvFilter, Layer, Registry, layer::SubscriberExt};

const LOG_LEVEL: &str = "debug";

pub(crate) fn setup(network: String, otlp_endpoint: Option<String>) {
    let mut layers: Box<dyn Layer<_> + Send + Sync + 'static> = Box::new(
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_filter(env_filter(LOG_LEVEL)),
    );

    let mut tracer_error: Option<anyhow::Error> = None;
    match init_tracer(network, otlp_endpoint) {
        Ok(tracer) => {
            layers = Box::new(
                layers.and_then(
                    tracing_opentelemetry::layer()
                        .with_tracer(tracer)
                        .with_filter(env_filter(LOG_LEVEL)),
                ),
            );
        }
        Err(err) => {
            tracer_error = Some(err);
        }
    };

    tracing::subscriber::set_global_default(Registry::default().with(layers))
        .unwrap_or_else(|e| panic!("Could not set tracing subscriber: {e}"));

    if let Some(tracer_error) = tracer_error {
        tracing::warn!("Could not create OpenTelemetry tracer: {tracer_error}");
    }
}

fn init_tracer(network: String, otlp_endpoint: Option<String>) -> Result<Tracer> {
    let otlp_endpoint = match otlp_endpoint {
        Some(endpoint) if endpoint.is_empty() => {
            return Err(anyhow!("OTLP endpoint is empty"));
        }
        Some(endpoint) => endpoint,
        None => {
            return Err(anyhow!("OTLP endpoint not provided"));
        }
    };

    tracing::info!("Enabling OpenTelemetry tracing");

    opentelemetry::global::set_text_map_propagator(
        opentelemetry_sdk::propagation::TraceContextPropagator::new(),
    );

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(otlp_endpoint)
        .with_compression(opentelemetry_otlp::Compression::Gzip)
        .build()?;

    let tracer = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_resource(
            opentelemetry_sdk::Resource::builder()
                .with_attributes(vec![
                    opentelemetry::KeyValue::new(
                        opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                        get_name(&network),
                    ),
                    opentelemetry::KeyValue::new("network", network),
                    opentelemetry::KeyValue::new(
                        opentelemetry_semantic_conventions::resource::SERVICE_VERSION,
                        built_info::PKG_VERSION,
                    ),
                    opentelemetry::KeyValue::new(
                        opentelemetry_semantic_conventions::resource::PROCESS_PID,
                        std::process::id().to_string(),
                    ),
                ])
                .build(),
        )
        .with_batch_exporter(exporter)
        .build();

    Ok(tracer.tracer(built_info::PKG_NAME))
}

fn env_filter(log_level: &str) -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(
            format!("{}={}", built_info::PKG_NAME, log_level)
                .parse()
                .unwrap(),
        )
        .with_env_var("CLN_PLUGIN_LOG")
        .from_env_lossy()
}

fn get_name(network: &str) -> String {
    format!("{}-{}", built_info::PKG_NAME, network)
}
