#!/usr/bin/env bash
# Step 1: build and start docker
podman compose up -d

# Step 2: create test torrent
./create_torrent.sh

# Step 3: optionally monitor logs
echo "Seeder logs:"
podman logs -f bt_seeder
