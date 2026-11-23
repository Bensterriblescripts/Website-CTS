#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$(readlink -f "$0")")/.."

host=${1:-cts}

cargo build --release --target x86_64-unknown-linux-musl

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

cp target/x86_64-unknown-linux-musl/release/cts "$stage/cts"
cp deploy/cts.service deploy/install.sh "$stage/"
tar -C public -cf "$stage/public.tar" .

tar -C "$stage" -czf - . | ssh "$host" 'rm -rf ~/.cts-deploy && mkdir -m 700 ~/.cts-deploy && tar -C ~/.cts-deploy -xzf -'
ssh "$host" 'sudo bash ~/.cts-deploy/install.sh; status=$?; rm -rf ~/.cts-deploy; exit $status'
