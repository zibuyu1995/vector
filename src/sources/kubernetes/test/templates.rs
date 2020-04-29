pub static CONFIG_MAP_YAML: &'static str = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: vector-config
data:
  vector.toml: |
    [sinks.out]
      type = "console"
      inputs = ["kubernetes"]
      target = "stdout"

      encoding = "json"
      healthcheck = true
"#;

pub static ECHO_YAML: &'static str = r#"
apiVersion: v1
kind: Pod
metadata:
  name: $(ECHO_NAME)
  namespace: $(TEST_NAMESPACE)
  labels:
    vector.test/label: echo
  annotations:
    vector.test/annotation: echo
spec:
  hostname: kube-test-node
  subdomain: echo
  containers:
  - name: $(ECHO_NAME)
    image: busybox:1.28
    command: ["echo"]
    args: ["$(ARGS_MARKER)"]
  restartPolicy: Never
"#;

pub static REPEATING_ECHO_YAML: &'static str = r#"
apiVersion: v1
kind: Pod
metadata:
  name: $(ECHO_NAME)
  namespace: $(TEST_NAMESPACE)
spec:
  containers:
  - name: $(ECHO_NAME)
    image: busybox:1.28
    command: ["sh"]
    args: ["-c","echo before; i=0; while [ $i -le 600 ]; do sleep 0.1; echo $(ARGS_MARKER); i=$((i+1)); done"]
  restartPolicy: Never
"#;
