#!/usr/bin/env python3
"""Test script to verify metrics are working"""

import requests

# Test the metrics endpoint
print("Testing metrics endpoint...")
metrics_url = "http://127.0.0.1:8005/metrics"
try:
    response = requests.get(metrics_url)
    if response.status_code == 200:
        print("✓ Metrics endpoint is accessible")

        # Check for key metrics
        metrics_text = response.text
        metrics_to_check = [
            "storage_write_duration",
            "storage_reads_total",
            "otel_scope_info",
        ]

        for metric in metrics_to_check:
            if metric in metrics_text:
                print(f"✓ Found metric: {metric}")
            else:
                print(f"✗ Missing metric: {metric}")

    else:
        print(f"✗ Metrics endpoint returned status: {response.status_code}")
except Exception as e:
    print(f"✗ Error accessing metrics: {e}")

# Test health endpoint
print("\nTesting health endpoint...")
health_url = "http://127.0.0.1:8005/health"
try:
    response = requests.get(health_url)
    if response.status_code == 200:
        print(f"✓ Health endpoint is accessible: {response.text.strip()}")
    else:
        print(f"✗ Health endpoint returned status: {response.status_code}")
except Exception as e:
    print(f"✗ Error accessing health: {e}")

# Show some actual metrics
print("\nSample metrics:")
try:
    response = requests.get(metrics_url)
    lines = response.text.split("\n")
    count = 0
    for line in lines:
        if line and not line.startswith("#") and ("storage" in line or "grpc" in line):
            print(f"  {line}")
            count += 1
            if count >= 10:
                break
except Exception as e:
    print(f"✗ Error reading metrics: {e}")
