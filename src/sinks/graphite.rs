use crate::{
    buffers::Acker,
    event::metric::MetricValue,
    sinks::tcp::TcpSink,
    sinks::util::SinkExt,
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
    Event,
};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::{future, Future, Sink};
use serde::{Deserialize, Serialize};
use snafu::ResultExt;
use std::net::{SocketAddr, ToSocketAddrs};
use tokio::net::TcpStream;

#[derive(Deserialize, Serialize, Debug, Clone)]
// TODO: add back when serde-rs/serde#1358 is addressed
// #[serde(deny_unknown_fields)]
pub struct GraphiteSinkConfig {
    #[serde(flatten)]
    pub mode: Mode,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Tcp { address: String },
    // Udp { address: SocketAddr },
}

impl GraphiteSinkConfig {
    pub fn new(mode: Mode) -> Self {
        Self { mode }
    }
}

inventory::submit! {
    SinkDescription::new_without_default::<GraphiteSinkConfig>("graphite")
}

#[typetag::serde(name = "graphite")]
impl SinkConfig for GraphiteSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let (sink, healthcheck) = match &self.mode {
            Mode::Tcp { address } => {
                let addr = address
                    .to_socket_addrs()
                    .context(super::SocketAddressError)?
                    .next()
                    .ok_or(Box::new(super::BuildError::DNSFailure {
                        address: address.clone(),
                    }))?;

                let sink = tcp(address.clone(), addr, cx.acker());

                let healthcheck = Box::new(future::lazy(move || {
                    TcpStream::connect(&addr)
                        .map(|_| ())
                        .map_err(|err| err.into())
                }));

                (sink, healthcheck)
            }
        };

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "graphite"
    }
}

fn tcp(hostname: String, addr: SocketAddr, acker: Acker) -> super::RouterSink {
    Box::new(
        TcpSink::new(hostname, addr, None)
            .stream_ack(acker)
            .with(encode_event),
    )
}

fn encode_timestamp(timestamp: Option<DateTime<Utc>>) -> i64 {
    if let Some(ts) = timestamp {
        ts.timestamp()
    } else {
        Utc::now().timestamp()
    }
}

fn encode_event(event: Event) -> Result<Bytes, ()> {
    let metric = event.into_metric();

    let ts = encode_timestamp(metric.timestamp);
    let value = match metric.value {
        MetricValue::Counter { value } => value,
        MetricValue::Gauge { value } => value,
        MetricValue::Set { values } => values.len() as f64,
        _ => 0.0,
    };

    let frame = format!("{} {} {}\n", metric.name, value, ts);
    let result = Bytes::from(frame);
    Ok(result)
}
