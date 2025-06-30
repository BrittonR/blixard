# Authentication Setup Guide

This guide explains how to configure authentication for secure Blixard cluster access control.

## Overview

Blixard supports token-based authentication with Role-Based Access Control (RBAC):
- **API Token Authentication** - Bearer tokens for clients and services
- **Permission-Based Access Control** - Fine-grained permissions for operations
- **Token Management** - Generation, validation, and expiration
- **Secure Secret Storage** - Encrypted storage for sensitive data

## Quick Start

### 1. Enable Authentication

#### Option A: Environment Variables
```bash
export BLIXARD_AUTH_ENABLED=true
export BLIXARD_AUTH_METHOD=token
export BLIXARD_AUTH_TOKEN_FILE=/etc/blixard/tokens.json
```

#### Option B: Configuration File
```toml
# /etc/blixard/config.toml
[security.auth]
enabled = true
method = "token"
token_file = "/etc/blixard/tokens.json"
```

### 2. Create Initial Tokens

Create a tokens file with initial admin access:
```json
{
  "5d41402abc4b2a76b9719d911017c592": {
    "token_hash": "5d41402abc4b2a76b9719d911017c592",
    "user": "admin",
    "permissions": ["Admin"],
    "expires_at": null,
    "created_at": "2024-01-01T00:00:00Z",
    "active": true
  }
}
```

> Note: The token hash is SHA256 of the actual token. For this example, the token is "hello".

### 3. Start Node with Authentication

```bash
blixard node --id 1 --bind 10.0.0.1:7001 --data-dir /var/lib/blixard/node1
```

### 4. Use Authenticated CLI

```bash
# Set authentication token
export BLIXARD_API_TOKEN=hello

# Or use Authorization header
blixard --header "Authorization: Bearer hello" cluster status

# Or use x-api-token header
blixard --header "x-api-token: hello" vm list
```

## Generating Tokens

### Using the CLI (when implemented)
```bash
# Generate admin token
blixard auth generate-token --user admin --role admin --expires 24h

# Generate read-only token
blixard auth generate-token --user viewer --permissions ClusterRead,VmRead --expires 7d

# Generate VM operator token
blixard auth generate-token --user vm-operator --role vm-operator
```

### Programmatically
```rust
use blixard_core::security::{SecurityManager, Permission};

let mut security_manager = SecurityManager::new(config).await?;

// Generate token with specific permissions
let token = security_manager.generate_token(
    "api-client",
    vec![Permission::VmRead, Permission::VmWrite],
    Some(Duration::from_secs(86400)), // 24 hours
).await?;

println!("Token: {}", token);
```

### Manual Token Creation

1. Generate a secure random token:
   ```bash
   openssl rand -hex 32
   ```

2. Hash the token:
   ```bash
   echo -n "your-token-here" | sha256sum | cut -d' ' -f1
   ```

3. Add to tokens file:
   ```json
   {
     "token_hash": {
       "token_hash": "hash-from-step-2",
       "user": "service-name",
       "permissions": ["VmRead", "VmWrite"],
       "expires_at": "2025-01-01T00:00:00Z",
       "created_at": "2024-01-01T00:00:00Z",
       "active": true
     }
   }
   ```

## Permission Model

### Available Permissions

- **Admin** - Full administrative access
- **ClusterRead** - Read cluster status and configuration
- **ClusterWrite** - Modify cluster configuration (join/leave)
- **VmRead** - List and view VM status
- **VmWrite** - Create, start, stop, delete VMs
- **TaskRead** - View task status
- **TaskWrite** - Submit and manage tasks
- **MetricsRead** - Access metrics and monitoring data

### Role Templates

#### Admin Role
```json
{
  "permissions": ["Admin"]
}
```

#### Read-Only Role
```json
{
  "permissions": ["ClusterRead", "VmRead", "TaskRead", "MetricsRead"]
}
```

#### VM Operator Role
```json
{
  "permissions": ["ClusterRead", "VmRead", "VmWrite", "TaskRead", "TaskWrite"]
}
```

#### Monitoring Role
```json
{
  "permissions": ["ClusterRead", "VmRead", "MetricsRead"]
}
```

## Integration Examples

### gRPC Client with Authentication

```rust
use tonic::transport::Channel;
use tonic::metadata::MetadataValue;

// Create authenticated channel
let mut channel = Channel::from_static("https://node1:7001")
    .connect()
    .await?;

// Add auth interceptor
let channel = tower::ServiceBuilder::new()
    .layer(InterceptedService::new(
        channel,
        AuthInterceptor::new("your-api-token"),
    ))
    .service(channel);

// Use client normally
let mut client = ClusterServiceClient::new(channel);
```

### HTTP API with Authentication

```bash
# Using curl
curl -H "Authorization: Bearer your-token-here" \
     https://node1:7001/api/v1/cluster/status

# Using wget
wget --header="x-api-token: your-token-here" \
     https://node1:7001/api/v1/vms
```

### Prometheus Scraping

For Prometheus to scrape metrics:

1. Create a metrics token:
   ```json
   {
     "prometheus_token_hash": {
       "user": "prometheus",
       "permissions": ["MetricsRead"],
       "active": true
     }
   }
   ```

2. Configure Prometheus:
   ```yaml
   scrape_configs:
     - job_name: 'blixard'
       bearer_token: 'prometheus-token-value'
       static_configs:
         - targets: ['node1:7001', 'node2:7001', 'node3:7001']
   ```

## Security Best Practices

### 1. Token Management
- Use strong, random tokens (minimum 32 bytes)
- Set reasonable expiration times
- Rotate tokens regularly
- Never commit tokens to version control
- Use different tokens for different services

### 2. Permission Assignment
- Follow principle of least privilege
- Create service-specific accounts
- Avoid using Admin permission unless necessary
- Regular audit of permissions

### 3. Token Storage
- Store token files with restricted permissions (0600)
- Use encrypted filesystems for token storage
- Consider using external secret management (Vault, KMS)
- Never log tokens in plaintext

### 4. Network Security
- Always use TLS with authentication
- Implement rate limiting for auth endpoints
- Monitor failed authentication attempts
- Use network segmentation

## Troubleshooting

### Common Issues

#### "No authentication token provided"
- Ensure token is set in environment or headers
- Check header format: "Bearer <token>" or direct token

#### "Invalid or expired token"
- Verify token exists in token file
- Check token hasn't expired
- Ensure token hash matches

#### "Insufficient permissions"
- Check user has required permissions
- Verify permission names are correct
- Consider if Admin permission is needed

### Debug Authentication

1. **Enable debug logging**
   ```bash
   export RUST_LOG=blixard=debug,blixard_core::security=trace
   ```

2. **Test token validation**
   ```bash
   # Check if token is recognized
   blixard auth validate --token your-token
   ```

3. **View authentication logs**
   ```bash
   journalctl -u blixard | grep -i auth
   ```

## Migration from No Authentication

To enable authentication on a running cluster:

1. **Prepare tokens** for all clients and services

2. **Enable auth in mixed mode** (temporary)
   ```toml
   [security.auth]
   enabled = true
   warn_only = true  # Log but don't enforce
   ```

3. **Update all clients** with tokens

4. **Enable enforcement**
   ```toml
   [security.auth]
   enabled = true
   warn_only = false  # Enforce authentication
   ```

## Advanced Configuration

### Custom Token Validation

Implement custom token validation:
```rust
impl TokenValidator for CustomValidator {
    async fn validate(&self, token: &str) -> Result<TokenInfo> {
        // Custom validation logic
        // e.g., check against external auth service
    }
}
```

### Integration with External Auth

- **OIDC/OAuth2** - Validate JWT tokens
- **LDAP/AD** - Map groups to permissions
- **Vault** - Dynamic token generation
- **AWS IAM** - Role-based authentication

### Audit Logging

Enable audit logging for security events:
```toml
[security.audit]
enabled = true
log_file = "/var/log/blixard/audit.log"
log_auth_success = true
log_auth_failure = true
log_permission_denied = true
```

## Performance Considerations

Authentication adds overhead:
- ~0.1-0.5ms per request for token validation
- Memory for token cache
- CPU for hash computation

Optimizations:
- Token caching with TTL
- Permission caching per session
- Batch token validation
- Use fast hashing (SHA256)