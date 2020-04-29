// NOTE: Tests assume that Kubernetes is accessable and localy available image of vector
//       that is to be tested is present.
#![cfg(feature = "kubernetes-integration-tests")]

use crate::{
    event,
    test_util::{random_string, trace_init, wait_for},
};
use kube::api::{Api, Log, LogParams};
use serde_json::Value;
use uuid::Uuid;

mod deleter;
mod deploy_util;
mod kube_util;
mod templates;
mod util;

pub use kube_util::KubeUtil as Kube;
use kube_util::*;
use templates::*;
use util::*;

static ARGS_MARKER: &'static str = "$(ARGS_MARKER)";
static ECHO_NAME: &'static str = "$(ECHO_NAME)";

pub fn user_namespace<S: AsRef<str>>(namespace: S) -> String {
    "user-".to_owned() + namespace.as_ref()
}

fn echo_create(template: &str, kube: &Kube, name: &str, message: &str) -> KubePod {
    kube.create(
        Api::v1Pod,
        template
            .replace(ECHO_NAME, name)
            .replace(ARGS_MARKER, format!("{}", message).as_str())
            .as_str(),
    )
}

#[must_use]
pub fn echo(kube: &Kube, name: &str, message: &str) -> KubePod {
    // Start echo
    let echo = echo_create(ECHO_YAML, kube, name, message);

    // Wait for success state
    kube.wait_for_success(echo.clone());

    echo
}

pub fn start_vector(
    namespace: impl Into<String>,
    extra: impl Into<String>,
) -> deploy_util::DeployUtil {
    deploy_util::DeployUtil::new(namespace, Some(extra))
}

pub fn logs(kube: &Kube) -> Vec<Value> {
    let mut logs = Vec::new();
    for daemon_instance in kube.list() {
        debug!(message="daemon_instance",name=%daemon_instance.metadata.name);
        logs.extend(
            kube.logs(daemon_instance.metadata.name.as_str())
                .into_iter()
                .filter_map(|s| serde_json::from_slice::<Value>(s.as_ref()).ok()),
        );
    }
    logs
}

pub fn info_vector_logs(kube: &Kube) {
    for daemon_instance in kube.list() {
        if let Ok(logs) = kube.api(Api::v1Pod).log(
            daemon_instance.metadata.name.as_str(),
            &LogParams::default(),
        ) {
            info!(
                "Deamon Vector's logs [{}]:\n{}",
                daemon_instance.metadata.name, logs
            );
        }
    }
}

#[test]
fn kube_one_log() {
    trace_init();
    let namespace = format!("one-log-{}", Uuid::new_v4());
    let message = random_string(300);
    let user_namespace = user_namespace(&namespace);

    let kube = Kube::new(&namespace);
    let user = Kube::new(&user_namespace);

    // Start vector
    let _vector_deployment = start_vector(&namespace, CONFIG_MAP_YAML);

    // Start echo
    let _echo = echo(&user, "echo", &message);

    // Verify logs
    // If any daemon logged message, done.
    wait_for(|| {
        for line in logs(&kube) {
            if line["message"].as_str().unwrap() == message {
                assert_eq!(
                    line[event::log_schema().source_type_key().as_ref()]
                        .as_str()
                        .unwrap(),
                    "kubernetes"
                );
                // DONE
                return true;
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        false
    });
}

#[test]
fn kube_old_log() {
    trace_init();
    let namespace = format!("old-log-{}", Uuid::new_v4());
    let message_old = random_string(300);
    let message_new = random_string(300);
    let user_namespace = user_namespace(&namespace);

    let kube = Kube::new(&namespace);
    let user = Kube::new(&user_namespace);

    // echo old
    let _echo_old = echo(&user, "echo-old", &message_old);

    // Start vector
    let _vector_deployment = start_vector(&namespace, CONFIG_MAP_YAML);

    // echo new
    let _echo_new = echo(&user, "echo-new", &message_new);

    // Verify logs
    wait_for(|| {
        let mut logged = false;
        for line in logs(&kube) {
            if line["message"].as_str().unwrap() == message_old {
                panic!("Old message logged");
            } else if line["message"].as_str().unwrap() == message_new {
                // OK
                logged = true;
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        logged
    });
}

#[test]
fn kube_multi_log() {
    trace_init();
    let namespace = format!("multi-log-{}", Uuid::new_v4());
    let mut messages = vec![
        random_string(300),
        random_string(300),
        random_string(300),
        random_string(300),
    ];
    let user_namespace = user_namespace(&namespace);

    let kube = Kube::new(&namespace);
    let user = Kube::new(&user_namespace);

    // Start vector
    let _vector_deployment = start_vector(&namespace, CONFIG_MAP_YAML);

    // Start echo
    let _echo = echo(&user, "echo", messages.join("\\n").as_str());

    // Verify logs
    wait_for(|| {
        for line in logs(&kube) {
            if Some(line["message"].as_str().unwrap()) == messages.first().map(|s| s.as_str()) {
                messages.remove(0);
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        messages.is_empty()
    });
}

#[test]
fn kube_object_uid() {
    trace_init();
    let namespace = "kube-object-uid".to_owned(); //format!("object-uid-{}", Uuid::new_v4());
    let message = random_string(300);
    let user_namespace = user_namespace(&namespace);

    let kube = Kube::new(&namespace);
    let user = Kube::new(&user_namespace);

    // Start vector
    let _vector_deployment = start_vector(&namespace, CONFIG_MAP_YAML);

    // Start echo
    let _echo = echo(&user, "echo", &message);
    // Verify logs
    wait_for(|| {
        // If any daemon has object uid, done.
        for line in logs(&kube) {
            if line.get("object_uid").is_some() {
                // DONE
                return true;
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        false
    });
}

#[test]
fn kube_diff_container() {
    trace_init();
    let namespace = format!("diff-container-{}", Uuid::new_v4());
    let message0 = random_string(300);
    let message1 = random_string(300);
    let user_namespace = user_namespace(&namespace);

    let kube = Kube::new(&namespace);
    let user = Kube::new(&user_namespace);

    // Start vector
    let _vector_deployment = start_vector(&namespace, CONFIG_MAP_YAML);

    // Start echo0
    let _echo0 = echo(&user, "echo0", &message0);
    let _echo1 = echo(&user, "echo1", &message1);

    // Verify logs
    // If any daemon logged message, done.
    wait_for(|| {
        for line in logs(&kube) {
            if line["message"].as_str().unwrap() == message1 {
                // DONE
                return true;
            } else if line["message"].as_str().unwrap() == message0 {
                panic!("Received message from not included container");
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        false
    });
}

#[test]
fn kube_diff_namespace() {
    trace_init();
    let namespace = format!("diff-namespace-{}", Uuid::new_v4());
    let message = random_string(300);
    let user_namespace0 = user_namespace(namespace.to_owned() + "0");
    let user_namespace1 = user_namespace(namespace.to_owned() + "1");

    let kube = Kube::new(&namespace);
    let user0 = Kube::new(&user_namespace0);
    let user1 = Kube::new(&user_namespace1);

    // Start vector
    let _vector_deployment = start_vector(&namespace, CONFIG_MAP_YAML);

    // Start echo0
    let _echo0 = echo(&user0, "echo", &message);
    let _echo1 = echo(&user1, "echo", &message);

    // Verify logs
    // If any daemon logged message, done.
    wait_for(|| {
        for line in logs(&kube) {
            if line["message"].as_str().unwrap() == message {
                if let Some(namespace) = line.get("pod_namespace") {
                    assert_eq!(namespace.as_str().unwrap(), user_namespace1);
                }
                // DONE
                return true;
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        false
    });
}

#[test]
fn kube_diff_pod_uid() {
    trace_init();
    let namespace = format!("diff-pod-uid-{}", Uuid::new_v4());
    let message = random_string(300);
    let user_namespace = user_namespace(&namespace);

    let kube = Kube::new(&namespace);
    let user = Kube::new(&user_namespace);

    // Start echo
    let echo0 = echo_create(REPEATING_ECHO_YAML, &user, "echo0", &message);
    let echo1 = echo_create(REPEATING_ECHO_YAML, &user, "echo1", &message);

    let uid0 = echo0.metadata.uid.as_ref().expect("UID present");
    let uid1 = echo1.metadata.uid.as_ref().expect("UID present");

    let mut uid = String::new();

    while uid0.starts_with(&uid) {
        uid = uid1.chars().take(uid.chars().count() + 1).collect();
    }

    // Start vector
    let _vector_deployment = start_vector(&namespace, CONFIG_MAP_YAML);

    // Verify logs
    wait_for(|| {
        // If any daemon logged message, done.
        for line in logs(&kube) {
            if line["message"].as_str().unwrap() == message {
                if let Some(uid) = line.get("object_uid") {
                    assert_eq!(uid.as_str().unwrap(), echo1.metadata.uid.as_ref().unwrap());
                } else if let Some(name) = line.get("container_name") {
                    assert_eq!(name.as_str().unwrap(), "echo1");
                }
                // DONE
                return true;
            } else {
                debug!(namespace=%namespace,log=%line);
            }
        }
        false
    });
}
