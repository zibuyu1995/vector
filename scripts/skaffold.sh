#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

cargo build

cat <<EOF >target/debug/.dockerignore
**/*
!vector
EOF

# shellcheck source=minikube-docker-env.sh disable=SC1091
. scripts/minikube-docker-env.sh

export SKAFFOLD_CACHE_ARTIFACTS=false

cargo watch -x build &
trap 'kill -- "-$$"; exit 0' INT

skaffold "$@"
