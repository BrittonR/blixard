# Certificate-Based Node Enrollment Setup

This guide demonstrates how to set up and use certificate-based enrollment for automatic node registration in Blixard.

## Overview

Certificate-based enrollment allows nodes to automatically receive appropriate roles and permissions based on their X.509 certificate attributes. This is ideal for:

- Automated cluster deployment
- Cloud-based auto-scaling
- Zero-touch node provisioning
- Service account authentication

## Configuration

### 1. Certificate Enrollment Configuration

Create a certificate enrollment configuration file (`cert-enrollment.json`):

```json
{
  "ca_cert_path": "/etc/blixard/ca-cert.pem",
  "cert_role_mappings": [
    {
      "cert_field": "CN",
      "pattern": "*.ops.blixard.io",
      "roles": ["admin", "operator"],
      "tenant": "operations"
    },
    {
      "cert_field": "OU",
      "pattern": "Development",
      "roles": ["developer", "viewer"],
      "tenant": null
    },
    {
      "cert_field": "CN",
      "pattern": "monitoring-*",
      "roles": ["monitoring", "viewer"],
      "tenant": "monitoring"
    },
    {
      "cert_field": "O",
      "pattern": "Blixard Inc",
      "roles": ["employee"],
      "tenant": null
    }
  ],
  "default_tenant": "default",
  "allow_self_signed": false
}
```

### 2. Cedar Policy Integration

The enrolled nodes automatically work with Cedar policies. Example policy for ops team:

```cedar
// Allow ops team full VM management
permit(
  principal in Role::"operator",
  action in [
    Action::"createVM",
    Action::"updateVM", 
    Action::"deleteVM",
    Action::"migrateVM"
  ],
  resource is VM
)
when {
  principal.tenant == resource.tenant
};

// Restrict VM deletion to maintenance windows
permit(
  principal in Role::"operator",
  action == Action::"deleteVM",
  resource is VM
)
when {
  context.time.hour >= 22 || context.time.hour <= 6
};
```

## Usage Examples

### Example 1: Operations Team Member

When an ops team member's node starts with certificate:
- CN: alice.ops.blixard.io
- OU: Operations
- O: Blixard Inc

The node automatically receives:
- User: alice.ops.blixard.io
- Roles: ["admin", "operator", "employee"]
- Tenant: operations

### Example 2: Development Environment

Developer nodes with certificate:
- CN: bob@blixard.io
- OU: Development
- O: Blixard Inc

Automatically receive:
- User: bob@blixard.io
- Roles: ["developer", "viewer", "employee"]
- Tenant: default

### Example 3: Monitoring Service

Service account with certificate:
- CN: monitoring-prometheus
- O: Blixard Inc

Automatically receives:
- User: monitoring-prometheus
- Roles: ["monitoring", "viewer", "employee"]
- Tenant: monitoring

## Token-Based Enrollment

For scenarios where certificates aren't available, use enrollment tokens:

### Generate Single-Use Operator Token
```bash
blixard security enroll generate \
  --user new-operator \
  --roles operator,monitoring \
  --tenant operations \
  --validity-hours 168
```

### Generate Multi-Use Worker Token
```bash
blixard security enroll generate \
  --user worker-pool \
  --roles worker \
  --tenant compute \
  --validity-hours 720 \
  --multi-use \
  --max-uses 50
```

### Enroll with Token
```bash
# On the node to be enrolled
blixard security enroll token \
  --token enroll_12345678-1234-1234-1234-123456789012 \
  --secret AbCdEfGhIjKlMnOpQrStUvWxYz123456
```

## Kubernetes Integration

For Kubernetes deployments, use the Pod Identity Webhook to inject certificates:

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: blixard-node
  annotations:
    blixard.io/enroll-cert-cn: "node-${HOSTNAME}.k8s.blixard.io"
    blixard.io/enroll-cert-ou: "Kubernetes"
spec:
  serviceAccountName: blixard-node
  containers:
  - name: blixard
    image: blixard:latest
    env:
    - name: BLIXARD_CERT_ENROLL_CONFIG
      value: /etc/blixard/cert-enrollment.json
    volumeMounts:
    - name: node-cert
      mountPath: /etc/blixard/certs
  volumes:
  - name: node-cert
    projected:
      sources:
      - serviceAccountToken:
          path: token
      - configMap:
          name: blixard-ca-cert
```

## Cloud Provider Integration

### AWS EC2 with Instance Identity

Use EC2 instance metadata to generate enrollment tokens:

```bash
#!/bin/bash
# User data script for EC2 instances

# Get instance metadata
INSTANCE_ID=$(ec2-metadata --instance-id | cut -d " " -f 2)
INSTANCE_TYPE=$(ec2-metadata --instance-type | cut -d " " -f 2)
AZ=$(ec2-metadata --availability-zone | cut -d " " -f 2)

# Request enrollment token from control plane
TOKEN_RESPONSE=$(curl -X POST https://blixard-control.internal/api/v1/enrollment/request \
  -H "X-AWS-Instance-Identity: $(curl -s http://169.254.169.254/latest/dynamic/instance-identity/document | base64)" \
  -d "{
    \"instance_id\": \"$INSTANCE_ID\",
    \"instance_type\": \"$INSTANCE_TYPE\",
    \"availability_zone\": \"$AZ\",
    \"requested_roles\": [\"worker\"]
  }")

TOKEN=$(echo $TOKEN_RESPONSE | jq -r .token)
SECRET=$(echo $TOKEN_RESPONSE | jq -r .secret)

# Enroll the node
blixard security enroll token --token $TOKEN --secret $SECRET
```

### GCP with Workload Identity

```bash
#!/bin/bash
# Startup script for GCE instances

# Get GCP metadata
INSTANCE_NAME=$(curl -H "Metadata-Flavor: Google" http://metadata.google.internal/computeMetadata/v1/instance/name)
SERVICE_ACCOUNT=$(curl -H "Metadata-Flavor: Google" http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/email)

# Use service account to get enrollment token
gcloud auth application-default login
TOKEN_RESPONSE=$(curl -X POST https://blixard-control.internal/api/v1/enrollment/gcp \
  -H "Authorization: Bearer $(gcloud auth print-access-token)" \
  -d "{
    \"instance_name\": \"$INSTANCE_NAME\",
    \"service_account\": \"$SERVICE_ACCOUNT\"
  }")

# Extract and use token
TOKEN=$(echo $TOKEN_RESPONSE | jq -r .token)
SECRET=$(echo $TOKEN_RESPONSE | jq -r .secret)
blixard security enroll token --token $TOKEN --secret $SECRET
```

## Security Best Practices

1. **Token Management**
   - Use single-use tokens for permanent nodes
   - Set appropriate expiration times
   - Rotate multi-use tokens regularly
   - Store tokens securely (e.g., HashiCorp Vault)

2. **Certificate Validation**
   - Always validate against trusted CA
   - Use certificate pinning for critical services
   - Implement certificate revocation checking
   - Monitor certificate expiration

3. **Audit Logging**
   - Log all enrollment attempts
   - Track token usage
   - Monitor for suspicious patterns
   - Alert on enrollment failures

4. **Network Security**
   - Use mTLS for enrollment API
   - Restrict enrollment endpoints
   - Implement rate limiting
   - Use IP allowlisting for control plane

## Troubleshooting

### Common Issues

1. **"No matching certificate role mappings"**
   - Check certificate attributes match patterns
   - Verify pattern wildcards are correct
   - Ensure cert_field names are standard (CN, OU, O, etc.)

2. **"Token expired"**
   - Generate new token with longer validity
   - Check system time synchronization
   - Consider using NTP

3. **"Invalid token secret"**
   - Verify token ID and secret match
   - Check for copy/paste errors
   - Ensure token hasn't been revoked

### Debug Commands

```bash
# List active tokens
blixard security enroll list

# Check node's current identity
blixard node identity

# Verify certificate attributes
openssl x509 -in node-cert.pem -text -noout | grep Subject:

# Test pattern matching
blixard security enroll test-pattern --pattern "*.ops.blixard.io" --value "alice.ops.blixard.io"
```

## Migration from Manual Configuration

To migrate existing manually-configured nodes:

1. Generate migration tokens for each node
2. Update node startup scripts to use enrollment
3. Gradually restart nodes with new configuration
4. Monitor enrollment success rate
5. Revoke old static credentials

Example migration script:
```bash
#!/bin/bash
# Generate tokens for all existing nodes
for node in $(blixard cluster status | jq -r '.nodes[].id'); do
  USER=$(blixard node get $node | jq -r .user)
  ROLES=$(blixard node get $node | jq -r .roles[])
  TENANT=$(blixard node get $node | jq -r .tenant)
  
  blixard security enroll generate \
    --user "$USER-migrated" \
    --roles "$ROLES" \
    --tenant "$TENANT" \
    --validity-hours 720 \
    --multi-use \
    --max-uses 1 | tee migration-token-$node.txt
done
```

This completes the certificate-based enrollment implementation with comprehensive examples and documentation!