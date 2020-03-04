use crate::expiring_hash_map::ExpiringHashMap;
use crate::{
    event::Event,
    sinks::util::SinkExt,
    template::Template,
    topology::config::{DataType, SinkConfig, SinkContext},
};
use bytes::Bytes;
use futures::compat::CompatSink;
use futures::future::{select, Either};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio02::{fs::File, io::AsyncWriteExt};

mod bytes_path;
use bytes_path::BytesPath;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct FileSinkConfig {
    pub path: Template,
    pub idle_timeout_secs: Option<u64>,
    pub encoding: Encoding,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Ndjson,
}

impl Default for Encoding {
    fn default() -> Self {
        Encoding::Text
    }
}

#[typetag::serde(name = "file")]
impl SinkConfig for FileSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = FileSink::new(&self).stream_ack(cx.acker());
        Ok((
            Box::new(CompatSink::new(sink)),
            Box::new(futures01::future::ok(())),
        ))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "file"
    }
}

#[derive(Debug, Default)]
pub struct FileSink {
    path: Template,
    encoding: Encoding,
    idle_timeout: Duration,
    files: ExpiringHashMap<Bytes, File>,
}

impl FileSink {
    pub fn new(config: &FileSinkConfig) -> Self {
        Self {
            path: config.path.clone(),
            idle_timeout_secs: config.idle_timeout_secs.unwrap_or(30),
            encoding: config.encoding.clone(),
            ..Default::default()
        }
    }

    /// Uses pass the `event` to `self.path` template to obtain the file path
    /// to store the event as.
    fn partition_event(&mut self, event: &Event) -> Option<bytes::Bytes> {
        let bytes = match self.path.render(event) {
            Ok(b) => b,
            Err(missing_keys) => {
                warn!(
                    message = "Keys do not exist on the event. Dropping event.",
                    ?missing_keys
                );
                return None;
            }
        };

        Some(bytes)
    }

    fn deadline_at(&self) -> Instant {
        Instant::now()
            .checked_add(self.idle_timeout)
            .expect("unable to compute next deadline")
    }

    async fn run(&mut self, mut input: impl Stream<Event>) -> Result<()> {
        loop {
            let what = select(input.next(), self.files.next()).await;
            match what {
                Either::Left(event) => {
                    // If we got `None` - terminate the processing.
                    let event = event?;

                    let path = match self.partition_event(event) {
                        Some(path) => path,
                        Err(err) => {
                            // We weren't able to find the path to use for the
                            // file.
                            // This is already logged at `partition_event`, so
                            // here we just skip the event.
                            continue;
                        }
                    };

                    let mut file = if let Some(file) = self.files.get_mut(&path) {
                        self.files.reset_at(&path, self.deadline_at());
                        file
                    } else {
                        let file = File::create(BytesPath::new(path.clone())).await.unwrap();
                        self.files
                            .insert_at(path.clone(), (file, self.deadline_at()));
                        self.files.get_mut(&path).unwrap()
                    };

                    // ...
                }
                Either::Right(expired) => {}
            }
        }
    }
}

impl Sink for FileSink {
    type SinkItem = Event;
    type SinkError = ();

    fn start_send(&mut self, event: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if let Some(key) = self.partition_event(&event) {
            let encoding = self.encoding.clone();

            let partition = self
                .partitions
                .entry(key.clone())
                .or_insert_with(|| File::new(key.clone(), encoding.clone()));

            self.last_accessed.insert(key.clone(), Instant::now());

            partition
                .start_send(event)
                .map_err(|error| error!(message = "Error writing to partition.", %error))
        } else {
            Ok(AsyncSink::Ready)
        }
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        let mut all_files_ready = false;

        for (file, partition) in &mut self.partitions {
            let ready = match partition.poll_complete() {
                Ok(Async::Ready(())) => true,
                Ok(Async::NotReady) => false,
                Err(error) => {
                    let file = String::from_utf8_lossy(&file[..]);
                    error!(message = "Unable to flush file.", %file, %error);
                    true
                }
            };

            all_files_ready = all_files_ready || ready;
        }

        for (key, last_accessed) in &self.last_accessed {
            if last_accessed.elapsed().as_secs() > self.idle_timeout_secs {
                if let Some(file) = self.partitions.remove(key) {
                    let file_path = String::from_utf8_lossy(&key[..]);
                    debug!(message = "Closing file.", file = %file_path);
                    self.closing.insert(key.clone(), file);
                }
            }
        }

        let mut closed_files = Vec::new();

        for (key, file) in &mut self.closing {
            if let Async::Ready(()) = file.close().unwrap() {
                closed_files.push(key.clone());
            }
        }

        for closed_file in closed_files {
            self.closing.remove(&closed_file);
        }

        // Set `next_linger_timeout` to the oldest file's elapsed time since last
        // write minus the idle_timeout.
        if let Some(min_last_accessed) = self
            .last_accessed
            .iter()
            .map(|(_, v)| v)
            .filter(|l| l.elapsed().as_secs() < self.idle_timeout_secs)
            .min()
        {
            let next_timeout = self.idle_timeout_secs - min_last_accessed.elapsed().as_secs();
            let linger_deadline = *min_last_accessed - std::time::Duration::from_secs(next_timeout);
            self.next_linger_timeout = Some(Delay::new(linger_deadline));
        }

        if let Some(next_linger) = &mut self.next_linger_timeout {
            next_linger
                .poll()
                .expect("This is a bug; we are always in a timer context");
        }

        // This sink has completely fnished when one of these is true in order:
        // 1. There are no active partitions and not files currently closing.
        // 2. All files have been flushed completely
        if (self.partitions.is_empty() && self.closing.is_empty()) || all_files_ready {
            Ok(Async::Ready(()))
        } else {
            Ok(Async::NotReady)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event,
        test_util::{
            lines_from_file, random_events_with_stream, random_lines_with_stream, temp_dir,
            temp_file,
        },
    };
    use futures01::stream;

    #[test]
    fn single_partition() {
        let template = temp_file();

        let config = FileSinkConfig {
            path: template.clone().into(),
            idle_timeout_secs: None,
            encoding: Encoding::Text,
        };

        let sink = FileSink::new(&config);
        let (input, events) = random_lines_with_stream(100, 64);

        let mut rt = crate::test_util::runtime();
        let pump = sink.send_all(events);
        let _ = rt.block_on(pump).unwrap();

        let output = lines_from_file(template);
        for (input, output) in input.into_iter().zip(output) {
            assert_eq!(input, output);
        }
    }

    #[test]
    fn many_partitions() {
        let directory = temp_dir();

        let mut template = directory.to_string_lossy().to_string();
        template.push_str("/{{level}}s-{{date}}.log");

        let config = FileSinkConfig {
            path: template.clone().into(),
            idle_timeout_secs: None,
            encoding: Encoding::Text,
        };

        let sink = FileSink::new(&config);

        let (mut input, _) = random_events_with_stream(32, 8);
        input[0].as_mut_log().insert("date", "2019-26-07");
        input[0].as_mut_log().insert("level", "warning");
        input[1].as_mut_log().insert("date", "2019-26-07");
        input[1].as_mut_log().insert("level", "error");
        input[2].as_mut_log().insert("date", "2019-26-07");
        input[2].as_mut_log().insert("level", "warning");
        input[3].as_mut_log().insert("date", "2019-27-07");
        input[3].as_mut_log().insert("level", "error");
        input[4].as_mut_log().insert("date", "2019-27-07");
        input[4].as_mut_log().insert("level", "warning");
        input[5].as_mut_log().insert("date", "2019-27-07");
        input[5].as_mut_log().insert("level", "warning");
        input[6].as_mut_log().insert("date", "2019-28-07");
        input[6].as_mut_log().insert("level", "warning");
        input[7].as_mut_log().insert("date", "2019-29-07");
        input[7].as_mut_log().insert("level", "error");

        let events = stream::iter_ok(input.clone().into_iter());
        let mut rt = crate::test_util::runtime();
        let pump = sink.send_all(events);
        let _ = rt.block_on(pump).unwrap();

        let output = vec![
            lines_from_file(&directory.join("warnings-2019-26-07.log")),
            lines_from_file(&directory.join("errors-2019-26-07.log")),
            lines_from_file(&directory.join("warnings-2019-27-07.log")),
            lines_from_file(&directory.join("errors-2019-27-07.log")),
            lines_from_file(&directory.join("warnings-2019-28-07.log")),
            lines_from_file(&directory.join("errors-2019-29-07.log")),
        ];

        assert_eq!(
            input[0].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[0][0])
        );
        assert_eq!(
            input[1].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[1][0])
        );
        assert_eq!(
            input[2].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[0][1])
        );
        assert_eq!(
            input[3].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[3][0])
        );
        assert_eq!(
            input[4].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[2][0])
        );
        assert_eq!(
            input[5].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[2][1])
        );
        assert_eq!(
            input[6].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[4][0])
        );
        assert_eq!(
            input[7].as_log()[&event::log_schema().message_key()],
            From::<&str>::from(&output[5][0])
        );
    }
}
