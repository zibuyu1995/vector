use k8s_openapi::api::core::v1::{PodSpec, PodStatus};
use kube::{
    api::{
        Api, DeleteParams, KubeObject, ListParams, Log, LogParams, Object, PostParams,
        PropagationPolicy,
    },
    client::APIClient,
    config,
};
use serde::de::DeserializeOwned;

use super::retry;

pub type KubePod = Object<PodSpec, PodStatus>;

pub struct KubeUtil {
    client: APIClient,
    namespace: String,
}

impl KubeUtil {
    pub fn new(namespace: &str) -> Self {
        let config = config::load_kube_config().expect("failed to load kubeconfig");
        let client = APIClient::new(config);
        let kube = Self {
            client,
            namespace: namespace.to_string(),
        };
        kube
    }

    pub fn api<K, F: FnOnce(APIClient) -> Api<K>>(&self, f: F) -> Api<K> {
        f(self.client.clone()).within(self.namespace.as_str())
    }

    /// Will substitute NAMESPACE_MARKER
    pub fn create<K, F: FnOnce(APIClient) -> Api<K>>(&self, f: F, yaml: &str) -> K
    where
        K: KubeObject + DeserializeOwned + Clone,
    {
        self.create_with(&self.api(f), yaml)
    }

    /// Will substitute NAMESPACE_MARKER
    fn create_with<K>(&self, api: &Api<K>, yaml: &str) -> K
    where
        K: KubeObject + DeserializeOwned + Clone,
    {
        let yaml = yaml.replace(NAMESPACE_MARKER, self.namespace.as_str());
        let map: serde_yaml::Value = serde_yaml::from_slice(yaml.as_bytes()).unwrap();
        let json = serde_json::to_vec(&map).unwrap();
        retry(|| {
            api.create(&PostParams::default(), json.clone())
                .map_err(|error| {
                    format!("Failed creating Kubernetes object with error: {:?}", error)
                })
        })
    }

    pub fn list<'a>(&self) -> Vec<KubePod> {
        retry(|| {
            self.api(Api::v1Pod)
                .list(&ListParams {
                    field_selector: Some(format!("metadata.namespace=={}", self.namespace)),
                    ..ListParams::default()
                })
                .map_err(|error| format!("Failed listing Pods with error: {:?}", error))
        })
        .items
        .into_iter()
        // Assume vector pod is called something like `vector-qwerty`.
        .filter(|item| item.metadata.name.as_str().starts_with("vector"))
        .collect()
    }

    pub fn logs(&self, pod_name: &str) -> Vec<String> {
        retry(|| {
            self.api(Api::v1Pod)
                .log(pod_name, &LogParams::default())
                .map_err(|error| format!("Failed getting Pod logs with error: {:?}", error))
        })
        .lines()
        .map(|s| s.to_owned())
        .collect()
    }

    pub fn wait_for_success(&self, mut object: KubePod) -> KubePod {
        let api = self.api(Api::v1Pod);
        let legal = ["Pending", "Running", "Succeeded"];
        let goal = "Succeeded";
        retry(move || {
            object = api
                .get_status(object.meta().name.as_str())
                .map_err(|error| format!("Failed getting object status with error: {:?}", error))?;
            match object.status.clone().ok_or("Object status is missing")? {
                PodStatus {
                    phase: Some(ref phase),
                    ..
                } if phase.as_str() == goal => Ok(object.clone()),
                PodStatus {
                    phase: Some(ref phase),
                    ..
                } if legal.contains(&phase.as_str()) => {
                    Err(format!("Pod in intermediate phase: {:?}", phase))
                }
                PodStatus { phase, .. } => {
                    Err(format!("Illegal pod phase with phase: {:?}", phase))
                }
            }
        })
    }

    pub fn cleanup(&self) {
        let _ = Api::v1Namespace(self.client.clone()).delete(
            self.namespace.as_str(),
            &DeleteParams {
                propagation_policy: Some(PropagationPolicy::Background),
                ..DeleteParams::default()
            },
        );
    }
}

impl Drop for KubeUtil {
    fn drop(&mut self) {
        self.cleanup();
    }
}
