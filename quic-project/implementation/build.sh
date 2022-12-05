#!/bin/bash

# Version
git rev-parse HEAD > VERSION

# Add all necessary files to the zip
# Do NOT remove the three scripts
zip -r artifact.zip VERSION setup-env.sh run-client.sh run-server.sh 
