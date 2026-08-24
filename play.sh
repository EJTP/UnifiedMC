#!/bin/sh
set -e
cd "$(dirname "$0")"
[ -f ./env.sh ] && . ./env.sh
exec .venv/bin/python unifiedmc.py play "${UNIFIEDMC_SERVER:?set UNIFIEDMC_SERVER in env.sh}" "$@"
