# TLS Setup Guide

This guide explains how to configure TLS and mutual TLS (mTLS) for secure Blixard cluster communication.

## Overview

Blixard supports TLS encryption for all network communications:
- **Node-to-node** communication (Raft consensus, peer connections)
- **Client-to-node** communication (CLI, API clients)
- **Mutual TLS** for strong authentication

## Quick Start

### 1. Generate Certificates

For development/testing:
```bash
cd blixard
./scripts/generate-certs.sh
source tls-env.sh
```

This creates:
- `certs/ca.crt` - Root CA certificate
- `certs/node{1,2,3}.{crt,key}` - Node certificates
- `certs/blixard-cli.{crt,key}` - CLI client certificate
- `certs/blixard-admin.{crt,key}` - Admin certificate

### 2. Configure Nodes

#### Option A: Environment Variables
```bash
# For each node
export BLIXARD_TLS_CA_CERT=/path/to/ca.crt
export BLIXARD_TLS_CERT=/path/to/node1.crt
export BLIXARD_TLS_KEY=/path/to/node1.key
export BLIXARD_TLS_REQUIRE_CLIENT_CERTS=true
```

#### Option B: Configuration File
```toml
# /etc/blixard/config.toml
[security.tls]
enabled = true
cert_file = "/etc/blixard/certs/node.crt"
key_file = "/etc/blixard/certs/node.key"
ca_file = "/etc/blixard/certs/ca.crt"
require_client_cert = true
```

### 3. Start Nodes with TLS

```bash
# Node 1
blixard node --id 1 --bind 10.0.0.1:7001 --data-dir /var/lib/blixard/node1

# Node 2  
blixard node --id 2 --bind 10.0.0.2:7001 --data-dir /var/lib/blixard/node2 \
  --join 10.0.0.1:7001

# Node 3
blixard node --id 3 --bind 10.0.0.3:7001 --data-dir /var/lib/blixard/node3 \
  --join 10.0.0.1:7001
```

### 4. Configure CLI Client

```bash
# Set client certificates
export BLIXARD_CLIENT_CA_CERT=/path/to/ca.crt
export BLIXARD_CLIENT_CERT=/path/to/blixard-cli.crt
export BLIXARD_CLIENT_KEY=/path/to/blixard-cli.key

# Use CLI
blixard --endpoint https://10.0.0.1:7001 cluster status
```

## Production Setup

### Certificate Requirements

Production certificates should:
- Use at least 2048-bit RSA or P-256 ECDSA keys
- Include proper Subject Alternative Names (SANs)
- Have reasonable validity periods (1 year max)
- Be issued by a trusted CA

### Certificate Generation with OpenSSL

#### 1. Create CA
```bash
# Generate CA key
openssl genrsa -out ca.key 4096

# Create CA certificate
openssl req -new -x509 -days 3650 -key ca.key -out ca.crt \
  -subj "/C=US/O=YourOrg/CN=Blixard Root CA"
```

#### 2. Create Node Certificate
```bash
# Generate node key
openssl genrsa -out node1.key 4096

# Create certificate request
openssl req -new -key node1.key -out node1.csr \
  -subj "/C=US/O=YourOrg/CN=node1.blixard.local"

# Create extensions file
cat > node1.ext <<EOF
subjectAltName = DNS:node1.blixard.local,DNS:*.blixard.local,IP:10.0.0.1
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth, clientAuth
EOF

# Sign certificate
openssl x509 -req -in node1.csr -CA ca.crt -CAkey ca.key \
  -CAcreateserial -out node1.crt -days 365 \
  -extfile node1.ext
```

### Using External PKI

For production, consider using:
- **Vault PKI**: HashiCorp Vault with PKI secrets engine
- **cert-manager**: Kubernetes native certificate management
- **AWS Private CA**: For AWS deployments
- **Let's Encrypt**: For public-facing endpoints

Example Vault integration:
```bash
# Enable PKI secrets engine
vault secrets enable pki

# Generate root CA
vault write pki/root/generate/internal \
  common_name="Blixard Root CA" \
  ttl=87600h

# Create role for node certificates
vault write pki/roles/blixard-node \
  allowed_domains="blixard.local" \
  allow_subdomains=true \
  max_ttl=8760h \
  key_usage="DigitalSignature,KeyEncipherment" \
  ext_key_usage="ServerAuth,ClientAuth"

# Generate node certificate
vault write pki/issue/blixard-node \
  common_name="node1.blixard.local" \
  alt_names="node1.blixard.local,node1" \
  ip_sans="10.0.0.1"
```

## Security Best Practices

### 1. Key Management
- Store private keys with restricted permissions (0600)
- Use hardware security modules (HSMs) for production
- Rotate certificates before expiration
- Never commit keys to version control

### 2. Certificate Validation
- Always verify certificate chains
- Check certificate expiration
- Validate Subject Alternative Names
- Pin CA certificates in production

### 3. TLS Configuration
```toml
[security.tls]
enabled = true
min_version = "1.2"  # Minimum TLS 1.2
cipher_suites = [    # Restrict to strong ciphers
    "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256",
    "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384",
    "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256",
    "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384"
]
```

### 4. Monitoring
- Monitor certificate expiration
- Alert on TLS handshake failures
- Track cipher suite usage
- Log authentication failures

## Troubleshooting

### Common Issues

#### "x509: certificate signed by unknown authority"
- Ensure CA certificate is correctly specified
- Verify CA certificate is in PEM format
- Check certificate chain is complete

#### "tls: bad certificate"
- Verify client certificate is valid for mTLS
- Check certificate has proper key usage
- Ensure certificate is not expired

#### "no certificate available"
- Confirm certificate and key files exist
- Check file permissions (readable by process)
- Verify PEM format is correct

### Debug TLS Issues

1. **Enable debug logging**
   ```bash
   export RUST_LOG=blixard=debug,rustls=debug
   ```

2. **Test with OpenSSL**
   ```bash
   # Test server certificate
   openssl s_client -connect node1:7001 -CAfile ca.crt
   
   # Test with client certificate
   openssl s_client -connect node1:7001 \
     -CAfile ca.crt \
     -cert client.crt \
     -key client.key
   ```

3. **Verify certificates**
   ```bash
   # Check certificate details
   openssl x509 -in node1.crt -text -noout
   
   # Verify certificate chain
   openssl verify -CAfile ca.crt node1.crt
   ```

## Migration from Insecure

To migrate a running cluster to TLS:

1. **Enable mixed mode** (temporary)
   ```toml
   [security.tls]
   enabled = true
   allow_insecure_fallback = true  # Temporary!
   ```

2. **Roll out certificates** to all nodes

3. **Update clients** with certificates

4. **Disable insecure mode**
   ```toml
   [security.tls]
   enabled = true
   require_client_cert = true
   allow_insecure_fallback = false
   ```

## Performance Considerations

TLS adds overhead:
- ~10-20% CPU increase for encryption
- ~1-5ms latency for handshakes
- Memory for session caches

Optimizations:
- Use ECDSA certificates (faster than RSA)
- Enable session resumption
- Use hardware acceleration (AES-NI)
- Consider TLS 1.3 for better performance