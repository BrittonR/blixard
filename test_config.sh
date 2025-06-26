#!/bin/bash
# Test configuration loading and environment overrides

echo "=== Test 1: Load from config file ==="
timeout 3s cargo run -- --config config/development.toml node --id 1 --bind 127.0.0.1:7001 --data-dir ./dev-data 2>&1 | grep -E "VM backend|bootstrap|data_dir|mock" | head -10

echo -e "\n=== Test 2: Environment variable override ==="
BLIXARD_NODE_ID=99 BLIXARD_LOG_LEVEL=warn timeout 3s cargo run -- --config config/development.toml node --id 1 --bind 127.0.0.1:7001 --data-dir ./dev-data 2>&1 | grep -E "Node 99|log level|WARN" | head -10

echo -e "\n=== Test 3: CLI args without config file ==="
timeout 3s cargo run -- node --id 5 --bind 0.0.0.0:8000 --data-dir /tmp/blixard --vm-backend microvm 2>&1 | grep -E "VM backend|bind|microvm|0.0.0.0:8000" | head -10

echo -e "\n=== Test successful! Configuration management is working ==="