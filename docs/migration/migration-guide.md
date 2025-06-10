# Migration Guide

This guide covers migrating from the current Gleam version of Blixard to the new Rust implementation, including compatibility considerations and success criteria.

## Migration Overview

The Rust version of Blixard is designed to be a drop-in replacement for the Gleam version, maintaining full compatibility while providing significant performance and reliability improvements.

## Migration from Gleam Version

### Data Migration

Export your existing service definitions and import them into the Rust version:

```bash
# Export current service definitions from Gleam version
gleam run -m service_manager -- export --format json > services.json

# Import into Rust version
blixard import --format json --file services.json --verify

# Verify migration success
blixard service list --compare-with services.json
```

### Zero-Downtime Migration Strategy

Follow these steps for a seamless migration without service interruption:

1. **Parallel Deployment**: Run Rust version alongside Gleam version
   ```bash
   # Start Rust cluster on different ports
   blixard node --id rust-1 --bind 127.0.0.1:8001
   blixard node --id rust-2 --bind 127.0.0.1:8002 --peer rust-1:127.0.0.1:8001
   ```

2. **Service Migration**: Move services one by one to Rust cluster
   ```bash
   # Export individual service
   gleam run -m service_manager -- export --service nginx > nginx.json
   
   # Import to Rust cluster
   blixard service import --file nginx.json
   
   # Verify service is running
   blixard service status nginx
   ```

3. **Validation**: Verify each service works correctly
   ```bash
   # Run health checks
   blixard service health nginx
   
   # Compare service behavior
   blixard service test nginx --compare-with gleam-cluster
   ```

4. **Cutover**: Switch CLI to point to Rust cluster
   ```bash
   # Update configuration
   export BLIXARD_CLUSTER=rust-cluster.example.com
   
   # Verify all operations work
   blixard service list
   ```

5. **Cleanup**: Decommission Gleam cluster
   ```bash
   # Stop Gleam services gracefully
   gleam run -m service_manager -- shutdown --graceful
   ```

## Backward Compatibility

The Rust implementation maintains full backward compatibility:

### CLI Commands
100% compatible with existing scripts and automation:
```bash
# These commands work identically
blixard service start nginx
blixard service stop nginx
blixard service list
blixard cluster status
```

### Configuration Format
Automatic conversion from old format:
```bash
# Old Gleam config is automatically converted
blixard config migrate --input gleam-config.toml --output rust-config.toml
```

### API Endpoints
RESTful API maintains compatibility:
```bash
# Existing API calls continue to work
curl http://cluster:8080/api/v1/services
curl -X POST http://cluster:8080/api/v1/services/nginx/start
```

### Data Formats
Automatic migration of stored data:
```bash
# Data is automatically migrated on first access
# No manual intervention required
```

## Feature Parity

The Rust version includes all features from the Gleam version plus additional capabilities:

### Existing Features (Maintained)
- ✅ Service lifecycle management (start, stop, restart)
- ✅ Cluster formation and discovery via Tailscale
- ✅ Distributed consensus with Raft
- ✅ Persistent state storage
- ✅ User and system service management
- ✅ CLI interface

### New Features (Added)
- ✅ MicroVM orchestration with microvm.nix
- ✅ Ceph storage integration
- ✅ Live VM migration
- ✅ Deterministic testing framework
- ✅ Enhanced monitoring and metrics
- ✅ RBAC and security features

## Migration Validation

### Automated Validation Tools

Use built-in validation tools to ensure successful migration:

```bash
# Run comprehensive migration validation
blixard migrate validate --gleam-export services.json

# Test service compatibility
blixard migrate test --service nginx --iterations 100

# Compare cluster behavior
blixard migrate compare --gleam-cluster gleam.local --rust-cluster rust.local
```

### Manual Validation Checklist

- [ ] All services imported successfully
- [ ] Service states match (running/stopped)
- [ ] Configuration converted correctly
- [ ] User permissions maintained
- [ ] Monitoring data continuous
- [ ] No data loss detected
- [ ] Performance meets or exceeds Gleam version

## Success Criteria and KPIs

### Technical Success Metrics

The migration is considered successful when these metrics are achieved:

- **Reliability**: 99.9%+ uptime in production environments
- **Performance**: 10x improvement in operation latency over Gleam version
- **Scalability**: Support for 100+ node clusters
- **Resource Efficiency**: 50% reduction in memory usage

### User Experience Metrics

- **Setup Time**: < 5 minutes to get first cluster running
- **Learning Curve**: Existing users can use new version immediately
- **Documentation**: Complete API documentation and tutorials
- **Community**: Active GitHub repository with regular releases

### Operational Metrics

- **Zero Security Incidents**: No consensus safety violations in production
- **Fast Recovery**: < 60 seconds MTTR for node failures
- **Efficient Updates**: Zero-downtime cluster upgrades
- **Comprehensive Monitoring**: Full observability into cluster health

## Migration Timeline

### Week 1-2: Preparation
- Export all service definitions
- Review and update any custom scripts
- Set up parallel test environment
- Train team on new features

### Week 3-4: Pilot Migration
- Migrate non-critical services first
- Monitor performance and stability
- Collect feedback and adjust

### Week 5-6: Production Migration
- Schedule maintenance windows
- Migrate critical services in phases
- Maintain rollback capability
- Monitor closely for issues

### Week 7-8: Optimization
- Tune performance settings
- Optimize resource allocation
- Complete documentation updates
- Decommission old infrastructure

## Rollback Plan

If issues arise during migration:

1. **Immediate Rollback**: Services can be moved back to Gleam cluster
   ```bash
   blixard service export --format gleam > rollback.gleam
   gleam run -m service_manager -- import --file rollback.gleam
   ```

2. **Partial Rollback**: Keep some services on each cluster
3. **Data Preservation**: All data is preserved in both directions

## Getting Help

### Resources
- Migration guide: This document
- API documentation: `/docs/api-reference.md`
- Community forum: GitHub discussions
- Enterprise support: Available for production migrations

### Common Issues
- **Import failures**: Check service definition format
- **Performance degradation**: Review resource allocation
- **Network issues**: Verify Tailscale configuration
- **Storage migration**: Ensure adequate disk space

## Post-Migration

After successful migration:

1. **Monitor closely** for the first week
2. **Optimize settings** based on actual usage
3. **Update documentation** and runbooks
4. **Share feedback** with the community
5. **Plan for new features** like VM orchestration

## See Also

- [Installation Guide](../operations/installation.md) - Setting up Rust version
- [Configuration Guide](../operations/configuration.md) - Configuration details
- [API Reference](../developer-guide/api-reference.md) - Complete API documentation