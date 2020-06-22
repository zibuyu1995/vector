use bytes05::{Bytes, BytesMut};
use futures01::{Async, Poll, Stream};
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{hash_map::Entry, HashMap, VecDeque};
use std::hash::Hash;
use std::time::Duration;
use tokio01::timer::DelayQueue;

#[derive(Debug, Hash, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// All consecutive lines matching this pattern are included in the group.
    /// The first line (the line that matched the start pattern) does not need
    /// to match the `ContinueThrough` pattern.
    /// This is useful in cases such as a Java stack trace, where some indicator
    /// in the line (such as leading whitespace) indicates that it is an
    /// extension of the preceeding line.
    ContinueThrough,

    /// All consecutive lines matching this pattern, plus one additional line,
    /// are included in the group.
    /// This is useful in cases where a log message ends with a continuation
    /// marker, such as a backslash, indicating that the following line is part
    /// of the same message.
    ContinuePast,

    /// All consecutive lines not matching this pattern are included in the
    /// group.
    /// This is useful where a log line contains a marker indicating that it
    /// begins a new message.
    HaltBefore,

    /// All consecutive lines, up to and including the first line matching this
    /// pattern, are included in the group.
    /// This is useful where a log line ends with a termination marker, such as
    /// a semicolon.
    HaltWith,
}

#[derive(Debug, Clone)]
pub(super) struct Config {
    /// Start pattern to look for as a beginning of the message.
    pub start_pattern: Regex,
    /// Condition pattern to look for. Exact behavior is configured via `mode`.
    pub condition_pattern: Regex,
    /// Mode of operation, specifies how the condition pattern is interpreted.
    pub mode: Mode,
}

pub struct LineAgg<T, K> {
    /// Configuration parameters to use.
    config: Config,
}

impl<T, K> LineAgg<T, K>
where
    K: Hash + Eq + Clone,
{
    pub fn new(config: Config) -> Self {
        Self {
            config,

            buffers: HashMap::new(),
        }
    }
}

impl<T, K> LineAgg<T, K>
where
    K: Hash + Eq + Clone,
{
    /// Handle line, if we have something to output - return it.
    fn handle_line(&mut self, line: Bytes, src: K) -> Option<(Bytes, K)> {
        // Check if we already have the buffered data for the source.
        match self.buffers.entry(src) {
            Entry::Occupied(mut entry) => {
                let condition_matched = self.config.condition_pattern.is_match(line.as_ref());
                match self.config.mode {
                    // All consecutive lines matching this pattern are included in
                    // the group.
                    Mode::ContinueThrough => {
                        if condition_matched {
                            let buffered = entry.get_mut();
                            add_next_line(buffered, line);
                            return None;
                        } else {
                            let buffered = entry.insert(line.as_ref().into());
                            return Some((buffered.freeze(), entry.key().clone()));
                        }
                    }
                    // All consecutive lines matching this pattern, plus one
                    // additional line, are included in the group.
                    Mode::ContinuePast => {
                        if condition_matched {
                            let buffered = entry.get_mut();
                            add_next_line(buffered, line);
                            return None;
                        } else {
                            let (src, mut buffered) = entry.remove_entry();
                            add_next_line(&mut buffered, line);
                            return Some((buffered.freeze(), src));
                        }
                    }
                    // All consecutive lines not matching this pattern are included
                    // in the group.
                    Mode::HaltBefore => {
                        if condition_matched {
                            let buffered = entry.insert(line.as_ref().into());
                            return Some((buffered.freeze(), entry.key().clone()));
                        } else {
                            let buffered = entry.get_mut();
                            add_next_line(buffered, line);
                            return None;
                        }
                    }
                    // All consecutive lines, up to and including the first line
                    // matching this pattern, are included in the group.
                    Mode::HaltWith => {
                        if condition_matched {
                            let (src, mut buffered) = entry.remove_entry();
                            add_next_line(&mut buffered, line);
                            return Some((buffered.freeze(), src));
                        } else {
                            let buffered = entry.get_mut();
                            add_next_line(buffered, line);
                            return None;
                        }
                    }
                }
            }
            Entry::Vacant(entry) => {
                // This line is a candidate for buffering, or passing through.
                if self.config.start_pattern.is_match(line.as_ref()) {
                    // It was indeed a new line we need to filter.
                    // Set the timeout and buffer this line.
                    self.timeouts
                        .insert(entry.key().clone(), self.config.timeout.clone());
                    entry.insert(line.as_ref().into());
                    return None;
                } else {
                    // It's just a regular line we don't really care about.
                    return Some((line, entry.into_key()));
                }
            }
        }
    }
}

fn add_next_line(buffered: &mut BytesMut, line: Bytes) {
    buffered.extend_from_slice(b"\n");
    buffered.extend_from_slice(&line);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_continue_through_1() {
        let lines = vec![
            "some usual line",
            "some other usual line",
            "first part",
            " second part",
            " last part",
            "another normal message",
            "finishing message",
            " last part of the incomplete finishing message",
        ];
        let config = Config {
            start_pattern: Regex::new("^[^\\s]").unwrap(),
            condition_pattern: Regex::new("^[\\s]+").unwrap(),
            mode: Mode::ContinueThrough,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![
            "some usual line",
            "some other usual line",
            concat!("first part\n", " second part\n", " last part"),
            "another normal message",
            concat!(
                "finishing message\n",
                " last part of the incomplete finishing message"
            ),
        ];
        run_and_assert(&lines, config, &expected);
    }

    #[test]
    fn mode_continue_past_1() {
        let lines = vec![
            "some usual line",
            "some other usual line",
            "first part \\",
            "second part \\",
            "last part",
            "another normal message",
            "finishing message \\",
            "last part of the incomplete finishing message \\",
        ];
        let config = Config {
            start_pattern: Regex::new("\\\\$").unwrap(),
            condition_pattern: Regex::new("\\\\$").unwrap(),
            mode: Mode::ContinuePast,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![
            "some usual line",
            "some other usual line",
            concat!("first part \\\n", "second part \\\n", "last part"),
            "another normal message",
            concat!(
                "finishing message \\\n",
                "last part of the incomplete finishing message \\"
            ),
        ];
        run_and_assert(&lines, config, &expected);
    }

    #[test]
    fn mode_halt_before_1() {
        let lines = vec![
            "INFO some usual line",
            "INFO some other usual line",
            "INFO first part",
            "second part",
            "last part",
            "ERROR another normal message",
            "ERROR finishing message",
            "last part of the incomplete finishing message",
        ];
        let config = Config {
            start_pattern: Regex::new("").unwrap(),
            condition_pattern: Regex::new("^(INFO|ERROR) ").unwrap(),
            mode: Mode::HaltBefore,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![
            "INFO some usual line",
            "INFO some other usual line",
            concat!("INFO first part\n", "second part\n", "last part"),
            "ERROR another normal message",
            concat!(
                "ERROR finishing message\n",
                "last part of the incomplete finishing message"
            ),
        ];
        run_and_assert(&lines, config, &expected);
    }

    #[test]
    fn mode_halt_with_1() {
        let lines = vec![
            "some usual line;",
            "some other usual line;",
            "first part",
            "second part",
            "last part;",
            "another normal message;",
            "finishing message",
            "last part of the incomplete finishing message",
        ];
        let config = Config {
            start_pattern: Regex::new("[^;]$").unwrap(),
            condition_pattern: Regex::new(";$").unwrap(),
            mode: Mode::HaltWith,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![
            "some usual line;",
            "some other usual line;",
            concat!("first part\n", "second part\n", "last part;"),
            "another normal message;",
            concat!(
                "finishing message\n",
                "last part of the incomplete finishing message"
            ),
        ];
        run_and_assert(&lines, config, &expected);
    }

    #[test]
    fn use_case_java_exception() {
        let lines = vec![
            "java.lang.Exception",
            "    at com.foo.bar(bar.java:123)",
            "    at com.foo.baz(baz.java:456)",
        ];
        let config = Config {
            start_pattern: Regex::new("^[^\\s]").unwrap(),
            condition_pattern: Regex::new("^[\\s]+at").unwrap(),
            mode: Mode::ContinueThrough,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![concat!(
            "java.lang.Exception\n",
            "    at com.foo.bar(bar.java:123)\n",
            "    at com.foo.baz(baz.java:456)",
        )];
        run_and_assert(&lines, config, &expected);
    }

    #[test]
    fn use_case_ruby_exception() {
        let lines = vec![
            "foobar.rb:6:in `/': divided by 0 (ZeroDivisionError)",
            "\tfrom foobar.rb:6:in `bar'",
            "\tfrom foobar.rb:2:in `foo'",
            "\tfrom foobar.rb:9:in `<main>'",
        ];
        let config = Config {
            start_pattern: Regex::new("^[^\\s]").unwrap(),
            condition_pattern: Regex::new("^[\\s]+from").unwrap(),
            mode: Mode::ContinueThrough,
            timeout: Duration::from_millis(10),
        };
        let expected = vec![concat!(
            "foobar.rb:6:in `/': divided by 0 (ZeroDivisionError)\n",
            "\tfrom foobar.rb:6:in `bar'\n",
            "\tfrom foobar.rb:2:in `foo'\n",
            "\tfrom foobar.rb:9:in `<main>'",
        )];
        run_and_assert(&lines, config, &expected);
    }

    #[test]
    fn legacy() {
        let lines = vec![
            "INFO some usual line",
            "INFO some other usual line",
            "INFO first part",
            "second part",
            "last part",
            "ERROR another normal message",
            "ERROR finishing message",
            "last part of the incomplete finishing message",
        ];
        let expected = vec![
            "INFO some usual line",
            "INFO some other usual line",
            concat!("INFO first part\n", "second part\n", "last part"),
            "ERROR another normal message",
            concat!(
                "ERROR finishing message\n",
                "last part of the incomplete finishing message"
            ),
        ];

        let stream = stream_from_lines(&lines);
        let line_agg = LineAgg::new(
            stream,
            Config::for_legacy(
                Regex::new("^(INFO|ERROR)").unwrap(), // example from the docs
                10,
            ),
        );
        let results = collect_results(line_agg);
        assert_results(results, &expected);
    }

    // Test helpers.

    /// Private type alias to be more expressive in the internal implementation.
    type Filename = String;

    fn stream_from_lines<'a>(
        lines: &'a [&'static str],
    ) -> impl Stream<Item = (Bytes, Filename), Error = ()> + 'a {
        futures01::stream::iter_ok::<_, ()>(
            lines
                .iter()
                .map(|line| (Bytes::from_static(line.as_bytes()), "test.log".to_owned())),
        )
    }

    fn collect_results<T, K>(line_agg: LineAgg<T, K>) -> Vec<(Bytes, K)>
    where
        T: Stream<Item = (Bytes, K), Error = ()>,
        K: Hash + Eq + Clone,
    {
        futures01::future::Future::wait(futures01::stream::Stream::collect(line_agg))
            .expect("Failed to collect test results")
    }

    fn assert_results(actual: Vec<(Bytes, Filename)>, expected: &[&'static str]) {
        let expected_mapped: Vec<(Bytes, Filename)> = expected
            .iter()
            .map(|line| (Bytes::from_static(line.as_bytes()), "test.log".to_owned()))
            .collect();

        assert_eq!(actual, expected_mapped);
    }

    fn run_and_assert(lines: &[&'static str], config: Config, expected: &[&'static str]) {
        let stream = stream_from_lines(lines);
        let line_agg = LineAgg::new(stream, config);
        let results = collect_results(line_agg);
        assert_results(results, expected);
    }
}
