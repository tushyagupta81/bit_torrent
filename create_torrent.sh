#!/usr/bin/env bash
mkdir -p torrent_files
# create a dummy file of 1MB for testing
dd if=/dev/urandom of=torrent_files/test_file.bin bs=1M count=1

# generate the torrent file
mktorrent -a "http://localhost:9091/announce" -o torrent_files/test_file.torrent torrent_files/test_file.bin

echo "Torrent file created at torrent_files/test_file.torrent"
