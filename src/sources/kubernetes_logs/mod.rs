//! This mod implements `kubernetes_logs` source.
//! The scope of this source is to consume the log files that `kubelet` keeps
//! at `/var/log/pods` at the host of the k8s node.

#![deny(missing_docs)]

mod k8s_paths_provider;
mod path_helpers;
mod parser;
mod partial_events_merger;
mod pod_metadata_annotator;

const FILE_KEY: &str = "file";
