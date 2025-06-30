# Runbook: P2P Image Verification Failures

## Alert Details
- **Alert Name**: P2PVerificationFailures
- **Severity**: Critical
- **Component**: P2P Image Distribution

## Description
P2P image verification is failing, indicating potential data corruption, tampering, or NAR hash mismatches. This is a critical security issue that could lead to compromised VM images being deployed.

## Impact
- **VM deployments blocked** - Images failing verification cannot be used
- **Security risk** - Potential deployment of tampered images
- **Distribution network reliability** - Trust in P2P system compromised
- **Performance degradation** - Fallback to slower distribution methods

## Diagnostic Steps

### 1. Identify Failing Images
```bash
# Check recent verification failures
journalctl -u blixard | grep -E "verification.*fail|hash.*mismatch" | tail -50

# Get P2P verification metrics
curl -s http://prometheus:9090/api/v1/query?query=rate(p2p_verification_failed[5m]) | jq

# List images with verification issues
blixard p2p image list --status failed-verification
```

### 2. Analyze Failure Types
```bash
# Check specific error messages
blixard p2p image verify <image-id> --verbose

# Common failure types:
# - NAR hash mismatch
# - Missing signature
# - Corrupted chunks
# - Incomplete transfer
# - Certificate issues
```

### 3. Verify Source Image Integrity
```bash
# On source node
SOURCE_NODE=$(blixard p2p image info <image-id> --format json | jq -r '.source_node')
ssh $SOURCE_NODE

# Verify original image
nix-store --verify-path /nix/store/<image-path>
nix-hash --type sha256 --base32 /nix/store/<image-path>

# Check image metadata
blixard p2p image inspect <image-id> --node $SOURCE_NODE
```

### 4. Check Network Transfer Issues
```bash
# Verify chunk integrity
blixard p2p chunks list --image <image-id> --status corrupted

# Check transfer logs
journalctl -u blixard | grep -E "p2p.*transfer.*<image-id>" | tail -100

# Network error statistics
ss -s | grep -i error
netstat -s | grep -i retrans
```

## Resolution Steps

### Scenario 1: Corrupted During Transfer
```bash
# 1. Clear corrupted chunks
blixard p2p cache clear --image <image-id> --corrupted-only

# 2. Retry transfer with verification
blixard p2p image pull <image-id> \
  --verify-chunks \
  --retry-corrupted \
  --max-retries 5

# 3. Use different source node if available
blixard p2p image pull <image-id> --prefer-node <alt-node>
```

### Scenario 2: Source Image Corrupted
```bash
# 1. Identify all nodes with this image
NODES=$(blixard p2p image locate <image-id> --format json | jq -r '.nodes[]')

# 2. Find a good copy
for node in $NODES; do
  echo "Checking $node..."
  if ssh $node "nix-store --verify-path /nix/store/<path>"; then
    echo "Good copy found on $node"
    GOOD_NODE=$node
    break
  fi
done

# 3. Re-distribute from good source
blixard p2p image redistribute <image-id> --source $GOOD_NODE

# 4. Mark corrupted sources
blixard p2p image mark-corrupted <image-id> --node <bad-node>
```

### Scenario 3: NAR Hash Mismatch
```bash
# 1. Regenerate NAR hash on source
ssh $SOURCE_NODE "nix-store --dump /nix/store/<path> | nix-hash --type sha256 --base32 /dev/stdin"

# 2. Update image metadata
blixard p2p image update-hash <image-id> \
  --nar-hash <new-hash> \
  --reason "Regenerated after corruption"

# 3. Trigger re-verification
blixard p2p image verify-all --image <image-id>
```

### Scenario 4: Deliberate Tampering Suspected
```bash
# SECURITY INCIDENT RESPONSE

# 1. Immediately quarantine the image
blixard p2p image quarantine <image-id> --reason "Suspected tampering"

# 2. Prevent usage
blixard vm block-image <image-id> --all-nodes

# 3. Collect forensic data
mkdir /tmp/incident-<image-id>
cd /tmp/incident-<image-id>

# Collect all versions
for node in $NODES; do
  ssh $node "tar -czf - /nix/store/<path>" > $node-image.tar.gz
  ssh $node "nix-store --dump /nix/store/<path> | sha256sum" > $node-hash.txt
done

# 4. Alert security team
blixard alert send --priority critical \
  --team security \
  --subject "Potential image tampering detected" \
  --image-id <image-id>
```

### Scenario 5: Certificate/Signature Issues
```bash
# 1. Check certificate validity
blixard p2p cert status
openssl x509 -in /etc/blixard/certs/p2p.crt -text -noout

# 2. Regenerate certificates if expired
blixard p2p cert regenerate --force

# 3. Re-sign affected images
for image in $(blixard p2p image list --unsigned --format json | jq -r '.images[].id'); do
  blixard p2p image sign $image
done

# 4. Distribute new certificates
blixard p2p cert distribute --all-nodes
```

## Immediate Mitigation

### Disable P2P Temporarily
```bash
# 1. Switch to direct distribution
blixard config set p2p.enabled false
blixard config set distribution.method direct

# 2. Clear P2P cache to prevent corrupted data usage
blixard p2p cache clear --all

# 3. Restart distribution service
systemctl restart blixard-distribution
```

### Enhanced Verification Mode
```bash
# Enable strict verification
blixard config set p2p.verification.strict true
blixard config set p2p.verification.parallel_chunks false
blixard config set p2p.verification.algorithm "sha256+blake3"

# Reduce chunk size for better corruption detection
blixard config set p2p.chunk_size "1MB"
```

## Recovery Verification

1. **No Verification Failures**
   ```bash
   # Should be 0
   curl -s http://prometheus:9090/api/v1/query?query=rate(p2p_verification_failed[5m]) | jq '.data.result[0].value[1]'
   ```

2. **All Images Verified**
   ```bash
   # Re-verify all cached images
   blixard p2p verify-all --report
   ```

3. **Transfer Success Rate**
   ```bash
   # Should be > 99%
   blixard p2p stats --metric transfer_success_rate
   ```

## Prevention

1. **Enhanced Verification Settings**
   ```toml
   # /etc/blixard/config.toml
   [p2p.verification]
   enabled = true
   algorithm = "sha256"
   verify_chunks = true
   verify_on_read = true
   parallel_verification = true
   failure_threshold = 3
   ```

2. **Network Reliability**
   ```bash
   # TCP tuning for reliability
   cat >> /etc/sysctl.conf <<EOF
   net.ipv4.tcp_sack = 1
   net.ipv4.tcp_timestamps = 1
   net.ipv4.tcp_window_scaling = 1
   net.core.default_qdisc = fq
   net.ipv4.tcp_congestion_control = bbr
   EOF
   ```

3. **Regular Integrity Checks**
   ```bash
   # Cron job for verification
   cat > /etc/cron.d/blixard-p2p-verify <<EOF
   0 */6 * * * root /usr/bin/blixard p2p verify-all --fix-corrupted
   EOF
   ```

4. **Secure Distribution**
   - Enable image signing
   - Use mutual TLS for P2P transfers
   - Implement allowlist for trusted sources
   - Regular security audits

## Security Considerations

1. **Incident Response**
   - Document all verification failures
   - Preserve corrupted images for analysis
   - Track image lineage and distribution paths

2. **Access Control**
   ```bash
   # Limit P2P image distribution
   blixard p2p policy create --name trusted-sources \
     --rule "source.verified && source.trusted_domain"
   ```

3. **Audit Trail**
   ```bash
   # Enable comprehensive P2P audit logging
   blixard config set p2p.audit.enabled true
   blixard config set p2p.audit.log_transfers true
   blixard config set p2p.audit.log_verifications true
   ```

## Escalation

For verification failures:
1. First occurrence: Investigate and fix
2. Repeated failures: Escalate to platform team
3. Suspected tampering: **Immediate security incident**

## Related Runbooks
- [P2P Transfer Failures](./p2p-failures.md)
- [High P2P Transfer Load](./p2p-congestion.md)
- [Security Incident Response](./security-incident.md)