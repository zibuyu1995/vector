#!/usr/bin/env bash
set -euo pipefail

# shellcheck disable=SC2181
! COMMANDS="$(minikube docker-env)"

# shellcheck disable=SC2181
if [[ "$?" != "0" ]]; then
  echo "Unable to obtain docker env from minikube; is minikube strated?" >&2
  exit 7
fi

eval "$COMMANDS"
