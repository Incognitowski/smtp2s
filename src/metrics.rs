use lazy_static::lazy_static;
use opentelemetry::{
    global,
    metrics::{Counter, Histogram, Meter, Unit},
};
use opentelemetry_sdk::metrics::MeterProvider;
use prometheus::{Encoder, Registry, TextEncoder};

lazy_static! {
    pub static ref METRICS_INSTANCE: Metrics = Metrics::new();
    pub static ref REGISTRY: Registry = prometheus::Registry::new();
}

pub struct Metrics {
    _meter: Meter,
    pub message_exchange_started: Counter<u64>,
    pub authorization_failed: Counter<u64>,
    pub message_processed_successfully: Counter<u64>,
    pub data_storage_timing: Histogram<f64>,
    pub attachments_stored: Counter<u64>,
}

impl Metrics {
    fn new() -> Self {
        let meter = global::meter("smtp2s");
        Self {
            _meter: meter.clone(),
            message_exchange_started: meter
                .u64_counter("message_exchange_started")
                .with_description("Counts the number of started SMTP sessions.")
                .init(),
            authorization_failed: meter
                .u64_counter("authorization_failed")
                .with_description("Counts the number of failed login attempts.")
                .init(),
            message_processed_successfully: meter
                .u64_counter("message_processed_successfully")
                .with_description("Counts the number of successfully processed emails.")
                .init(),
            data_storage_timing: meter
                .f64_histogram("data_storage_timing")
                .with_description("Measures the duration of the email saving process.")
                .with_unit(Unit::new("s"))
                .init(),
            attachments_stored: meter
                .u64_counter("attachments_stored")
                .with_description("Counts the number of stored attachments.")
                .init(),
        }
    }
}

pub fn setup_metrics_provider() {
    let exporter = opentelemetry_prometheus::exporter()
        .with_registry(REGISTRY.clone())
        .build()
        .unwrap();
    let provider = MeterProvider::builder().with_reader(exporter).build();
    global::set_meter_provider(provider);
}

pub fn gather_metrics() -> String {
    let mut buffer = vec![];
    let encoder = TextEncoder::new();
    encoder
        .encode(REGISTRY.gather().as_slice(), &mut buffer)
        .unwrap();
    String::from_utf8(buffer).unwrap()
}
