#!/bin/bash

apt install -y zip
# Build client & server
cargo build --release

# Version
git rev-parse HEAD > VERSION

# Add all necessary files to the zip
# Do NOT remove the three scripts
zip -r artifact.zip VERSION setup-env.sh run-client.sh run-server.sh target/release/client target/release/server
