use kube::{
    api::{DeleteParams, PropagationPolicy, RawApi},
    client::APIClient,
};

pub struct Deleter {
    client: APIClient,
    name: String,
    api: RawApi,
}

impl Drop for Deleter {
    fn drop(&mut self) {
        let _ = self
            .api
            .delete(
                self.name.as_str(),
                &DeleteParams {
                    propagation_policy: Some(PropagationPolicy::Background),
                    ..DeleteParams::default()
                },
            )
            .and_then(|request| self.client.request_text(request))
            .map_err(|error| error!(message = "Failed deleting Kubernetes object.",%error));
    }
}
