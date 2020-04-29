use std::process::Command;

/// Environment variable which contains the name of the command to use to deploy
/// Vector into the kubernetes cluster.
/// Expected interface:
/// - Invocation example: command [up|down] [namespace]
/// - Deploys `vector` to the specified namespace.
/// - Reads additional `ResourceConfig`s from the file passed via
///   the `CUSTOM_RESOURCE_CONIFGS_FILE` env var.
/// - Exits with 0 upon success.
/// - Prints errors and warnings to stderr.
static KUBE_TEST_DEPLOY_COMMAND_ENV: &'static str = "KUBE_TEST_DEPLOY_COMMAND";
static CUSTOM_RESOURCE_CONIFGS_FILE_ENV: &'static str = "CUSTOM_RESOURCE_CONIFGS_FILE";

#[derive(Debug)]
pub struct DeployUtil {
    command: String,
    namespace: String,
    extra_resource_config: Option<String>,
}

impl DeployUtil {
    pub fn new(
        namespace: impl Into<String>,
        extra_resource_config: Option<impl Into<String>>,
    ) -> Self {
        let command = std::env::var(KUBE_TEST_DEPLOY_COMMAND_ENV).expect(
            format!(
                "{} environment variable must be set with the test deploment utility command.",
                KUBE_TEST_DEPLOY_COMMAND_ENV
            )
            .as_str(),
        );
        let namespace = namespace.into();
        let extra_resource_config = extra_resource_config.map(Into::into);
        let mut kube = Self {
            command,
            namespace,
            extra_resource_config,
        };
        kube.up();
        kube
    }

    fn build_cmd(&mut self, mode: &str) -> Command {
        let mut command = Command::new(&self.command);
        command.args(&[mode, self.namespace.as_str()]);

        if let Some(ref cfg) = self.extra_resource_config {
            let filename = {
                let mut tmpdir = std::env::temp_dir();
                tmpdir.push("k8s-extra-data.yaml");
                tmpdir
            };
            std::fs::write(&filename, cfg).expect("unable to write the file wih extra config");
            command.env(CUSTOM_RESOURCE_CONIFGS_FILE_ENV, filename);
        }

        trace!("Running command: {:?}", &command);

        command
    }

    fn up(&mut self) {
        self.build_cmd("up").output().expect("failed to deploy");
    }

    fn down(&mut self) {
        self.build_cmd("down").output().expect("failed to teradown");
    }

    pub fn namespace(&self) -> &str {
        self.namespace.as_str()
    }
}

impl Drop for DeployUtil {
    fn drop(&mut self) {
        self.down()
    }
}
