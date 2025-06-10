# Blixard Usage Scenarios

## Scenario 0: Serverless Functions (Lambda-like)

Deploy and manage serverless functions with AWS Lambda-like capabilities on your NixOS cluster:

```bash
# Define a serverless function
cat > image-processor.nix <<EOF
{ pkgs, ... }: {
  # Minimal microVM config for fast startup
  microvm = {
    mem = 128;  # Can be as low as needed
    vcpu = 1;
    bootTime = "50ms";  # Optimized for speed
  };
  
  # Function handler
  handler = { event, context }: 
    let
      image = pkgs.imagemagick.processImage event.imagePath;
    in {
      statusCode = 200;
      body = {
        processed = true;
        outputPath = image.path;
      };
    };
}
EOF

# Deploy serverless function
blixard function create image-processor --config image-processor.nix \
  --memory 128M --timeout 30s --concurrent-executions 100

# Function automatically scales to zero when idle
blixard function list
# Output: image-processor (0 instances running, 0 warm)

# Invoke function (cold start ~80ms)
blixard function invoke image-processor --data '{"imagePath": "/data/input.jpg"}'
# Function runs on optimal node based on:
# - Available CPU/memory
# - Proximity to /data/input.jpg if using Ceph
# - Network latency to caller

# After invocation, function stays warm briefly
blixard function list  
# Output: image-processor (1 instance warm on nixos-node3)

# Create HTTP trigger
blixard function trigger image-processor --http --path /api/process-image
# Now accessible at: https://cluster.local/api/process-image

# View function metrics
blixard function metrics image-processor
# Output: Invocations: 1.2k/hour, Avg cold start: 82ms, Avg duration: 450ms
```

## Scenario 1: Web Application Deployment (NixOS-Native)

Deploy a web application using pure NixOS configuration:

```bash
# Create VM definition using NixOS modules
cat > webapp.nix <<EOF
{ pkgs, modulesPath, ... }: {
  imports = [ (modulesPath + "/profiles/qemu-guest.nix") ];
  
  microvm = {
    mem = 512;
    vcpu = 2;
    balloonMem = 256;
    shares = [{
      source = "/nix/store";
      mountPoint = "/nix/.ro-store";
      tag = "ro-store";
      proto = "virtiofs";
    }];
  };
  
  # Pure NixOS service configuration
  services.nginx = {
    enable = true;
    virtualHosts.default = {
      root = "/var/www";
      locations."/".index = "index.html";
    };
  };
  
  networking.firewall.allowedTCPPorts = [ 80 ];
  
  # Declarative user management
  users.users.webapp = {
    isNormalUser = true;
    home = "/var/www";
  };
}
EOF

# Deploy across NixOS cluster with Ceph storage
blixard vm create webapp --config webapp.nix --replicas 3 \
  --storage-pool ssd-pool --network web-vlan
blixard vm start webapp
blixard vm status webapp
# Output: webapp VMs running on [nixos-node1, nixos-node2, nixos-node3]

# Scale with automatic placement
blixard vm scale webapp --replicas 5
# Creates VMs using Nix store sharing for efficiency

# Rolling update with NixOS generations
blixard vm update webapp --config webapp-v2.nix --strategy rolling
# Uses NixOS atomic upgrades + live migration
```

## Scenario 2: Database Cluster Management (NixOS-Native)

Deploy a PostgreSQL cluster with full NixOS integration:

```bash
# Create PostgreSQL cluster using NixOS modules
cat > postgres.nix <<EOF
{ pkgs, config, modulesPath, ... }: {
  imports = [ (modulesPath + "/profiles/qemu-guest.nix") ];
  
  microvm = {
    mem = 2048;
    vcpu = 4;
    shares = [{
      source = "/nix/store";
      mountPoint = "/nix/.ro-store";
      tag = "ro-store";
      proto = "virtiofs";
    }];
  };
  
  # Pure NixOS PostgreSQL configuration
  services.postgresql = {
    enable = true;
    package = pkgs.postgresql15;
    dataDir = "/var/lib/postgresql/15";
    settings = {
      shared_preload_libraries = [ "pg_stat_statements" ];
      max_connections = 200;
      shared_buffers = "256MB";
    };
    authentication = ''
      local all all trust
      host all all 0.0.0.0/0 md5
    '';
  };
  
  # Declarative database setup
  services.postgresql.initialScript = pkgs.writeText "init.sql" ''
    CREATE DATABASE app_db;
    CREATE USER app_user WITH PASSWORD 'secure_password';
    GRANT ALL PRIVILEGES ON DATABASE app_db TO app_user;
  '';
  
  # Ceph RBD mount for persistent data
  fileSystems."/var/lib/postgresql" = {
    device = "ceph-pool/postgres-vol";
    fsType = "ext4";
    options = [ "defaults" "noatime" ];
  };
  
  networking.firewall.allowedTCPPorts = [ 5432 ];
}
EOF

# Deploy with Ceph replication and NixOS reproducibility
blixard vm create postgres --config postgres.nix --replicas 3 \
  --storage-class replicated --storage-size 100G \
  --primary-election-policy "oldest-first"
blixard vm start postgres

# NixOS atomic failover with storage migration
# When primary VM fails, Blixard:
# 1. Promotes secondary VM to primary using NixOS activation
# 2. Mounts Ceph volume on new primary
# 3. Updates load balancer configuration
blixard vm status postgres
# Output: postgres primary=nixos-postgres-2, replicas=[nixos-postgres-2, nixos-postgres-3]
```

## Scenario 3: Complete Microservices Platform (Mixed Long-lived + Serverless)

Deploy a complete platform mixing traditional services with serverless functions:

```bash
# Deploy platform mixing long-lived services and serverless functions
cat > platform.yaml <<EOF
apiVersion: blixard.dev/v1
kind: Platform
metadata:
  name: ecommerce-platform
spec:
  # Long-lived services
  vms:
    - name: postgres-db
      type: long-lived
      config: ./configs/postgres.nix
      replicas: 3
      storage: { class: "ssd", size: "100G" }
      placement: { nodeSelector: { storage: "fast" } }
    
    - name: redis-cache
      type: long-lived  
      config: ./configs/redis.nix
      replicas: 2
      memory: 4096
      placement: { spreadAcross: "availability-zones" }
  
  # Serverless functions
  functions:
    - name: order-processor
      config: ./functions/order-processor.nix
      memory: 256
      timeout: 60s
      triggers:
        - type: queue
          source: orders-queue
      scaling:
        minInstances: 0  # Scale to zero
        maxInstances: 100
        targetConcurrency: 1
    
    - name: image-resizer
      config: ./functions/image-resizer.nix  
      memory: 512
      timeout: 30s
      triggers:
        - type: http
          path: /api/resize
      placement:
        # Run on any node, prefer those with GPU
        nodeAffinity: { preferred: { gpu: "available" } }
    
    - name: pdf-generator
      config: ./functions/pdf-generator.nix
      memory: 1024
      timeout: 120s
      triggers:
        - type: event
          source: invoice-created
      # This function can run on ANY node in the cluster
      # Blixard will choose optimal placement at runtime
EOF

blixard apply -f platform.yaml
# Creates long-lived VMs with fixed placement AND serverless functions

# Monitor mixed platform
blixard dashboard --stack ecommerce-platform
# Shows: 
# - Long-lived VMs: postgres (3/3 running), redis (2/2 running)
# - Serverless functions: order-processor (0 running, 2 warm), 
#   image-resizer (5 running on nodes 2,3,5), pdf-generator (0 running)

# Watch serverless scaling in action
blixard function watch order-processor
# Shows real-time scaling as orders come in, placement decisions,
# and which nodes are selected for each invocation
```