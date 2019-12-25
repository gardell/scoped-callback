#!/usr/bin/env bash
set -o errexit -o nounset -o pipefail -o xtrace

cargo readme | sed -E -r 's/(\[[^]]+\])\(([^\)]+)\)/\1(https:\/\/docs.rs\/scoped-callback\/latest\/\2)/'
