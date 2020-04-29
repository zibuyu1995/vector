static WAIT_LIMIT: usize = 120; //s

/// If F returns None, retries it after some time, for some count.
/// Panics if all trys fail.
pub fn retry<F: FnMut() -> Result<R, E>, R, E: std::fmt::Debug>(mut f: F) -> R {
    let mut last_error = None;
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(WAIT_LIMIT as u64) {
        match f() {
            Ok(data) => return data,
            Err(error) => {
                error!(?error);
                last_error = Some(error);
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        debug!("Retrying");
    }
    panic!("Timed out while waiting. Last error: {:?}", last_error);
}

pub fn create_namespace(&kube: super::KubeUtil) {
    kube.create_with(&Api::v1Namespace(kube.client.clone()), NAMESPACE_YAML);
}

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
            .map_err(|error| format!("Failed creating Kubernetes object with error: {:?}", error))
    })
}
