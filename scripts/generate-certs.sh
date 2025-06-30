#!/bin/bash
#
# Generate TLS certificates for Blixard mutual TLS
#
# This script generates:
# - Root CA certificate
# - Server certificates for nodes
# - Client certificates for CLI/API access

set -euo pipefail

# Configuration
CERT_DIR="${CERT_DIR:-./certs}"
VALIDITY_DAYS="${VALIDITY_DAYS:-365}"
KEY_SIZE="${KEY_SIZE:-4096}"
COUNTRY="${COUNTRY:-US}"
STATE="${STATE:-CA}"
LOCALITY="${LOCALITY:-San Francisco}"
ORG="${ORG:-Blixard}"
OU="${OU:-Platform}"

# Create certificate directory
mkdir -p "$CERT_DIR"
cd "$CERT_DIR"

echo "=== Generating Blixard TLS Certificates ==="
echo "Output directory: $CERT_DIR"
echo

# Generate CA private key
echo "1. Generating CA private key..."
openssl genrsa -out ca.key $KEY_SIZE

# Generate CA certificate
echo "2. Generating CA certificate..."
openssl req -new -x509 -days $VALIDITY_DAYS -key ca.key -out ca.crt \
    -subj "/C=$COUNTRY/ST=$STATE/L=$LOCALITY/O=$ORG/OU=$OU/CN=Blixard Root CA"

# Function to generate node certificate
generate_node_cert() {
    local node_name=$1
    local node_ip=${2:-"127.0.0.1"}
    local node_dns=${3:-"localhost"}
    
    echo "Generating certificate for node: $node_name"
    
    # Generate private key
    openssl genrsa -out "${node_name}.key" $KEY_SIZE
    
    # Create certificate config
    cat > "${node_name}.conf" <<EOF
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
C = $COUNTRY
ST = $STATE
L = $LOCALITY
O = $ORG
OU = $OU
CN = $node_name

[v3_req]
keyUsage = keyEncipherment, dataEncipherment, digitalSignature
extendedKeyUsage = serverAuth, clientAuth
subjectAltName = @alt_names

[alt_names]
DNS.1 = $node_name
DNS.2 = $node_dns
DNS.3 = *.blixard.local
IP.1 = $node_ip
IP.2 = 127.0.0.1
EOF
    
    # Generate certificate request
    openssl req -new -key "${node_name}.key" -out "${node_name}.csr" \
        -config "${node_name}.conf"
    
    # Sign certificate with CA
    openssl x509 -req -in "${node_name}.csr" -CA ca.crt -CAkey ca.key \
        -CAcreateserial -out "${node_name}.crt" -days $VALIDITY_DAYS \
        -extensions v3_req -extfile "${node_name}.conf"
    
    # Clean up
    rm "${node_name}.csr" "${node_name}.conf"
    
    # Create combined PEM for convenience
    cat "${node_name}.key" "${node_name}.crt" > "${node_name}.pem"
}

# Generate node certificates
echo
echo "3. Generating node certificates..."
generate_node_cert "node1" "10.0.0.1" "node1.blixard.local"
generate_node_cert "node2" "10.0.0.2" "node2.blixard.local"
generate_node_cert "node3" "10.0.0.3" "node3.blixard.local"

# Generate client certificate for CLI
echo
echo "4. Generating client certificate..."
generate_node_cert "blixard-cli" "127.0.0.1" "cli.blixard.local"

# Generate admin certificate
echo
echo "5. Generating admin certificate..."
generate_node_cert "blixard-admin" "127.0.0.1" "admin.blixard.local"

# Set permissions
echo
echo "6. Setting file permissions..."
chmod 600 *.key
chmod 644 *.crt *.pem

# Create convenience symlinks
ln -sf node1.crt node.crt
ln -sf node1.key node.key
ln -sf blixard-cli.crt client.crt
ln -sf blixard-cli.key client.key

# Display certificate info
echo
echo "=== Certificate Generation Complete ==="
echo
echo "CA Certificate:"
openssl x509 -in ca.crt -noout -subject -dates
echo
echo "Node1 Certificate:"
openssl x509 -in node1.crt -noout -subject -dates -ext subjectAltName
echo

# Create example environment file
cat > ../tls-env.sh <<'EOF'
# Source this file to configure TLS for Blixard

# Server-side TLS configuration
export BLIXARD_TLS_CA_CERT="$(pwd)/certs/ca.crt"
export BLIXARD_TLS_CERT="$(pwd)/certs/node.crt"
export BLIXARD_TLS_KEY="$(pwd)/certs/node.key"
export BLIXARD_TLS_REQUIRE_CLIENT_CERTS=true
export BLIXARD_TLS_VERIFY_HOSTNAME=true

# Client-side TLS configuration
export BLIXARD_CLIENT_CA_CERT="$(pwd)/certs/ca.crt"
export BLIXARD_CLIENT_CERT="$(pwd)/certs/client.crt"
export BLIXARD_CLIENT_KEY="$(pwd)/certs/client.key"

echo "TLS environment configured. Certificates in: $(pwd)/certs/"
EOF

echo "To use these certificates, run:"
echo "  source tls-env.sh"
echo
echo "For production, use a proper PKI infrastructure!"