#!/usr/bin/env bash
set -o errexit -o nounset -o pipefail -o xtrace

echo "![](https://github.com/gardell/scoped-callback/workflows/CI/badge.svg)"
echo "[![Docs](https://docs.rs/scoped-callback/badge.svg)](https://docs.rs/scoped-callback/latest/scoped_callback)"
cargo readme | sed -E -r 's/(\[[^]]+\])\(([^\)]+)\)/\1(https:\/\/docs.rs\/scoped-callback\/latest\/\2)/'
