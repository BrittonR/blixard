#!/bin/bash
# Script to generate test certificates for a Blixard cluster

set -e

echo "🔐 Generating test certificates for Blixard cluster..."

# Create certificate directory
mkdir -p ./test-certs

# Generate certificates for a 3-node cluster
cargo run --bin blixard -- security gen-certs \
    --cluster-name test-cluster \
    --nodes node1,node2,node3 \
    --clients admin,operator,monitoring \
    --output-dir ./test-certs \
    --validity-days 365 \
    --key-algorithm ecdsa-p256

echo ""
echo "✅ Certificates generated successfully!"
echo ""
echo "📂 Certificate files:"
ls -la ./test-certs/

echo ""
echo "🔍 To verify a certificate:"
echo "openssl x509 -in ./test-certs/server-node1.crt -text -noout"