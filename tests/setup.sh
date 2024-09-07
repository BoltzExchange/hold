#!/bin/bash
set -x

script_dir=$(dirname -- "$(readlink -f -- "$0")")
cargo_toml_path="$script_dir/../Cargo.toml"
version=$(awk -F'=' '/^\[package\]/ { in_package = 1 } in_package && /version/ { gsub(/[" ]/, "", $2); print $2; exit }' "$cargo_toml_path")

artifact_url="https://github.com/BoltzExchange/hold/releases/download/v$version/hold-linux-amd64.tar.gz"
archive_file="$script_dir/hold.tar.gz"

if ! curl -L "$artifact_url" -o "$archive_file"; then
  exit 1
fi

if ! tar -xzvf "$archive_file" -C "$script_dir"; then
  exit 1
fi
