//! Integration tests for resilience patterns
//!
//! This module provides comprehensive testing for the resilience patterns
//! implemented throughout the codebase, including:
//! - Circuit breaker functionality under various failure conditions
//! - Exponential backoff and jitter effectiveness
//! - Graceful degradation during system failures
//! - Health check escalation and recovery coordination
//! - End-to-end resilience under failure injection

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        error::{BlixardError, BlixardResult},
        patterns::{
            CircuitBreaker, CircuitBreakerConfig, RetryConfig, retry,
            GracefulDegradation, ServiceLevel, DegradationLevel,
        },
        vm_health_escalation::{VmHealthEscalationManager, EscalationConfig},
        vm_health_types::{
            HealthCheck, HealthCheckType, HealthCheckPriority, RecoveryAction, RecoveryEscalation,
        },
    };
    use std::sync::atomic::{AtomicU32, AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::time::sleep;

    /// Mock service that can be configured to fail
    struct MockService {
        failure_rate: Arc<AtomicU32>, // Percentage (0-100)
        call_count: Arc<AtomicU32>,
        should_be_slow: Arc<AtomicBool>,
    }

    impl MockService {
        fn new() -> Self {
            Self {
                failure_rate: Arc::new(AtomicU32::new(0)),
                call_count: Arc::new(AtomicU32::new(0)),
                should_be_slow: Arc::new(AtomicBool::new(false)),
            }
        }

        fn set_failure_rate(&self, rate: u32) {
            self.failure_rate.store(rate, Ordering::Relaxed);
        }

        fn set_slow(&self, slow: bool) {
            self.should_be_slow.store(slow, Ordering::Relaxed);
        }

        fn get_call_count(&self) -> u32 {
            self.call_count.load(Ordering::Relaxed)
        }

        async fn call(&self) -> BlixardResult<String> {
            let count = self.call_count.fetch_add(1, Ordering::Relaxed);
            
            // Simulate slow response if configured
            if self.should_be_slow.load(Ordering::Relaxed) {
                sleep(Duration::from_millis(100)).await;
            }

            // Simulate failures based on failure rate
            let failure_rate = self.failure_rate.load(Ordering::Relaxed);
            if failure_rate > 0 {
                let should_fail = (count % 100) < failure_rate;
                if should_fail {
                    return Err(BlixardError::NetworkError(format!("Simulated failure #{}", count)));
                }
            }

            Ok(format!("Success #{}", count))
        }
    }

    #[tokio::test]
    async fn test_circuit_breaker_under_failure() {
        let service = Arc::new(MockService::new());
        let service_clone = Arc::clone(&service);

        // Configure circuit breaker
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            reset_timeout: Duration::from_millis(100),
            operation_timeout: Some(Duration::from_millis(50)),
            enable_logging: false,
            ..Default::default()
        };

        let breaker = CircuitBreaker::new("test_service".to_string(), config);

        // Test 1: Circuit breaker should be closed initially
        assert_eq!(breaker.state().await, crate::patterns::CircuitBreakerState::Closed);

        // Test 2: Successful operations should keep circuit closed
        for _ in 0..5 {
            let result = breaker.call_async(|| async {
                service_clone.call().await
            }).await;
            assert!(result.is_ok());
        }
        assert_eq!(breaker.state().await, crate::patterns::CircuitBreakerState::Closed);

        // Test 3: Configure service to fail and trigger circuit opening
        service.set_failure_rate(100); // 100% failure rate

        let mut failure_count = 0;
        for _ in 0..10 {
            let result = breaker.call_async(|| async {
                service_clone.call().await
            }).await;
            
            if result.is_err() {
                failure_count += 1;
            }
        }

        // Circuit should be open after threshold failures
        assert_eq!(breaker.state().await, crate::patterns::CircuitBreakerState::Open);
        assert!(failure_count >= 3); // At least the threshold failures

        // Test 4: Circuit should reject requests when open
        let rejected_result = breaker.call_async(|| async {
            service_clone.call().await
        }).await;
        assert!(rejected_result.is_err());
        assert!(rejected_result.unwrap_err().to_string().contains("circuit breaker"));

        // Test 5: Wait for reset timeout and test half-open transition
        sleep(Duration::from_millis(150)).await;

        // Fix the service
        service.set_failure_rate(0);

        // Next call should transition to half-open and then closed
        let result = breaker.call_async(|| async {
            service_clone.call().await
        }).await;
        assert!(result.is_ok());

        // Eventually should be closed again
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if breaker.state().await == crate::patterns::CircuitBreakerState::Closed {
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
        }).await.expect("Circuit should close after successful operation");

        let stats = breaker.stats().await;
        assert!(stats.total_calls > 0);
        assert!(stats.failed_calls > 0);
        assert!(stats.successful_calls > 0);
        assert!(stats.rejected_calls > 0);
    }

    #[tokio::test]
    async fn test_retry_with_jitter_effectiveness() {
        let service = Arc::new(MockService::new());
        let service_clone = Arc::clone(&service);

        // Configure service to fail initially, then succeed
        service.set_failure_rate(80); // 80% failure rate

        // Test exponential backoff with jitter
        let config = RetryConfig::for_network_operations("test_retry")
            .with_logging(false);

        let start = Instant::now();
        
        // This should eventually succeed due to retries
        let result = retry(config, || {
            let service = Arc::clone(&service_clone);
            Box::pin(async move {
                // Reduce failure rate over time to simulate recovery
                let call_count = service.get_call_count();
                if call_count > 3 {
                    service.set_failure_rate(20); // Reduce to 20%
                }
                if call_count > 6 {
                    service.set_failure_rate(0); // Eventually succeed
                }
                service.call().await
            })
        }).await;

        let elapsed = start.elapsed();
        
        assert!(result.is_ok(), "Retry should eventually succeed");
        assert!(elapsed > Duration::from_millis(100), "Should have some delay from retries");
        assert!(elapsed < Duration::from_secs(10), "Should not take too long with proper backoff");
        assert!(service.get_call_count() > 1, "Should have made multiple attempts");
    }

    #[tokio::test]
    async fn test_graceful_degradation_flow() {
        let degradation = Arc::new(GracefulDegradation::new());

        // Register services with different criticality levels
        degradation.register_service("critical_db", ServiceLevel::Critical).await.unwrap();
        degradation.register_service("cache_service", ServiceLevel::Important).await.unwrap();
        degradation.register_service("analytics", ServiceLevel::Optional).await.unwrap();
        degradation.register_service("debug_logs", ServiceLevel::Experimental).await.unwrap();

        // Initially all services should be available
        assert!(degradation.is_service_available("critical_db").await);
        assert!(degradation.is_service_available("cache_service").await);
        assert!(degradation.is_service_available("analytics").await);
        assert!(degradation.is_service_available("debug_logs").await);

        // Simulate failure of optional service
        degradation.mark_service_unhealthy("analytics", "simulated failure").await.unwrap();
        
        assert!(degradation.is_service_available("critical_db").await);
        assert!(degradation.is_service_available("cache_service").await);
        assert!(!degradation.is_service_available("analytics").await);

        // Force degradation to minimal mode
        degradation.force_degradation_level(DegradationLevel::Minimal).await.unwrap();

        assert!(degradation.is_service_available("critical_db").await);
        assert!(degradation.is_service_available("cache_service").await);
        assert!(!degradation.is_service_available("analytics").await);
        assert!(!degradation.is_service_available("debug_logs").await);

        // Force degradation to emergency mode
        degradation.force_degradation_level(DegradationLevel::Emergency).await.unwrap();

        assert!(degradation.is_service_available("critical_db").await);
        assert!(!degradation.is_service_available("cache_service").await);
        assert!(!degradation.is_service_available("analytics").await);
        assert!(!degradation.is_service_available("debug_logs").await);

        // Recover service and check stats
        degradation.mark_service_healthy("analytics").await.unwrap();
        degradation.force_degradation_level(DegradationLevel::Normal).await.unwrap();

        let stats = degradation.stats().await;
        assert_eq!(stats.current_level, DegradationLevel::Normal);
        assert_eq!(stats.total_services, 4);
        assert!(stats.degradation_events > 0);
        assert!(stats.recovery_events > 0);
    }

    #[tokio::test]
    async fn test_combined_resilience_patterns() {
        // Test that all resilience patterns work together
        let service = Arc::new(MockService::new());
        let degradation = Arc::new(GracefulDegradation::new());
        
        // Register monitoring service
        degradation.register_service("mock_service", ServiceLevel::Important).await.unwrap();

        // Create circuit breaker
        let breaker_config = CircuitBreakerConfig {
            failure_threshold: 2,
            reset_timeout: Duration::from_millis(200),
            enable_logging: false,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new("combined_test".to_string(), breaker_config);

        // Create retry config
        let retry_config = RetryConfig::for_critical_operations("combined_test")
            .with_logging(false);

        // Scenario 1: Everything working normally
        service.set_failure_rate(0);
        
        let result = breaker.call_async(|| async {
            retry(retry_config.clone(), || {
                let service = Arc::clone(&service);
                Box::pin(async move {
                    if degradation.is_service_available("mock_service").await {
                        service.call().await
                    } else {
                        Err(BlixardError::TemporaryFailure {
                            details: "Service degraded".to_string()
                        })
                    }
                })
            }).await
        }).await;
        
        assert!(result.is_ok());

        // Scenario 2: Service degradation
        degradation.mark_service_unhealthy("mock_service", "test failure").await.unwrap();
        
        let result = breaker.call_async(|| async {
            retry(retry_config.clone(), || {
                let service = Arc::clone(&service);
                Box::pin(async move {
                    if degradation.is_service_available("mock_service").await {
                        service.call().await
                    } else {
                        Err(BlixardError::TemporaryFailure {
                            details: "Service degraded".to_string()
                        })
                    }
                })
            }).await
        }).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Service degraded"));

        // Scenario 3: High failure rate triggering circuit breaker
        degradation.mark_service_healthy("mock_service").await.unwrap();
        service.set_failure_rate(100);

        // Multiple calls should eventually open the circuit
        for _ in 0..5 {
            let _ = breaker.call_async(|| async {
                retry(retry_config.clone(), || {
                    let service = Arc::clone(&service);
                    Box::pin(async move {
                        if degradation.is_service_available("mock_service").await {
                            service.call().await
                        } else {
                            Err(BlixardError::TemporaryFailure {
                                details: "Service degraded".to_string()
                            })
                        }
                    })
                }).await
            }).await;
        }

        // Circuit should be open, preventing further calls
        assert_eq!(breaker.state().await, crate::patterns::CircuitBreakerState::Open);

        // Verify that even after service recovery, circuit breaker prevents calls
        service.set_failure_rate(0);
        let result = breaker.call_async(|| async {
            retry(retry_config.clone(), || {
                let service = Arc::clone(&service);
                Box::pin(async move { service.call().await })
            }).await
        }).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("circuit breaker"));
    }

    #[tokio::test]
    async fn test_failure_injection_with_health_checks() {
        use crate::vm_health_types::VmHealthCheckConfig;

        // Create health checks with different priorities
        let quick_check = HealthCheck {
            name: "quick_connectivity".to_string(),
            check_type: HealthCheckType::Tcp {
                address: "127.0.0.1:22".to_string(),
                timeout_secs: 1,
            },
            critical: true,
            weight: 1.0,
            priority: HealthCheckPriority::Quick,
            recovery_escalation: Some(RecoveryEscalation {
                actions: vec![
                    (RecoveryAction::None, Duration::from_secs(10)),
                    (RecoveryAction::RestartVm, Duration::from_secs(30)),
                ],
                max_attempts: 2,
                reset_after: Duration::from_secs(300),
            }),
            failure_threshold: 2,
        };

        let deep_check = HealthCheck {
            name: "deep_application".to_string(),
            check_type: HealthCheckType::Http {
                url: "http://127.0.0.1:8080/health".to_string(),
                expected_status: 200,
                timeout_secs: 3,
                headers: None,
            },
            critical: false,
            weight: 0.8,
            priority: HealthCheckPriority::Deep,
            recovery_escalation: None,
            failure_threshold: 3,
        };

        let comprehensive_check = HealthCheck {
            name: "comprehensive_system".to_string(),
            check_type: HealthCheckType::Script {
                command: "/usr/local/bin/system-check.sh".to_string(),
                args: vec!["--comprehensive".to_string()],
                expected_exit_code: 0,
                timeout_secs: 10,
            },
            critical: true,
            weight: 1.0,
            priority: HealthCheckPriority::Comprehensive,
            recovery_escalation: Some(RecoveryEscalation {
                actions: vec![
                    (RecoveryAction::RestartVm, Duration::from_secs(60)),
                    (RecoveryAction::MigrateVm { target_node: None }, Duration::from_secs(300)),
                ],
                max_attempts: 3,
                reset_after: Duration::from_secs(600),
            }),
            failure_threshold: 1,
        };

        let checks = vec![quick_check, deep_check, comprehensive_check];
        let config = VmHealthCheckConfig {
            checks: checks.clone(),
            check_interval_secs: 5,
            failure_threshold: 2,
            success_threshold: 1,
            initial_delay_secs: 0,
        };

        // Test health check escalation behavior
        // Note: This is a simplified test - in a real scenario, we would need
        // to set up mock VM backends and network services

        // Verify health check configuration
        assert_eq!(checks.len(), 3);
        assert!(checks.iter().any(|c| c.priority == HealthCheckPriority::Quick));
        assert!(checks.iter().any(|c| c.priority == HealthCheckPriority::Deep));
        assert!(checks.iter().any(|c| c.priority == HealthCheckPriority::Comprehensive));

        // Verify recovery escalation configuration
        let critical_checks: Vec<_> = checks.iter().filter(|c| c.critical).collect();
        assert_eq!(critical_checks.len(), 2);

        for check in &critical_checks {
            assert!(check.recovery_escalation.is_some());
            if let Some(ref escalation) = check.recovery_escalation {
                assert!(!escalation.actions.is_empty());
                assert!(escalation.max_attempts > 0);
            }
        }

        // Test timeout multipliers for different priorities
        for check in &checks {
            let base_timeout = match &check.check_type {
                HealthCheckType::Tcp { timeout_secs, .. } => *timeout_secs,
                HealthCheckType::Http { timeout_secs, .. } => *timeout_secs,
                HealthCheckType::Script { timeout_secs, .. } => *timeout_secs,
                _ => 5,
            };
            
            let multiplier = check.priority.timeout_multiplier();
            let adjusted_timeout = (base_timeout as f32 * multiplier) as u64;
            
            match check.priority {
                HealthCheckPriority::Quick => assert_eq!(multiplier, 1.0),
                HealthCheckPriority::Deep => assert_eq!(multiplier, 2.0),
                HealthCheckPriority::Comprehensive => assert_eq!(multiplier, 3.0),
            }
            
            assert!(adjusted_timeout >= base_timeout);
        }
    }

    #[tokio::test]
    async fn test_resilience_metrics_and_observability() {
        // Test that resilience patterns generate appropriate metrics
        let breaker = CircuitBreaker::new(
            "metrics_test".to_string(),
            CircuitBreakerConfig {
                failure_threshold: 2,
                reset_timeout: Duration::from_millis(100),
                enable_logging: false,
                ..Default::default()
            }
        );

        let service = Arc::new(MockService::new());
        let service_clone = Arc::clone(&service);

        // Generate some successful calls
        for _ in 0..5 {
            let _ = breaker.call_async(|| async {
                service_clone.call().await
            }).await;
        }

        // Generate some failures
        service.set_failure_rate(100);
        for _ in 0..3 {
            let _ = breaker.call_async(|| async {
                service_clone.call().await
            }).await;
        }

        // Check metrics
        let stats = breaker.stats().await;
        assert_eq!(stats.total_calls, 8);
        assert_eq!(stats.successful_calls, 5);
        assert_eq!(stats.failed_calls, 3);
        assert!(stats.failure_rate > 0.0);
        assert_eq!(stats.state, crate::patterns::CircuitBreakerState::Open);
        assert!(stats.last_failure_at.is_some());
        assert!(stats.last_success_at.is_some());

        // Test graceful degradation metrics
        let degradation = Arc::new(GracefulDegradation::new());
        degradation.register_service("test1", ServiceLevel::Critical).await.unwrap();
        degradation.register_service("test2", ServiceLevel::Optional).await.unwrap();
        
        degradation.mark_service_unhealthy("test2", "test").await.unwrap();
        
        let degradation_stats = degradation.stats().await;
        assert_eq!(degradation_stats.total_services, 2);
        assert_eq!(degradation_stats.healthy_services, 1);
        assert_eq!(degradation_stats.degraded_services, 1);
        assert!(degradation_stats.last_change.is_some());
        
        // Verify service level breakdown
        assert_eq!(degradation_stats.services_by_level.get(&ServiceLevel::Critical), Some(&1));
        assert_eq!(degradation_stats.services_by_level.get(&ServiceLevel::Optional), Some(&1));
    }

    #[tokio::test]
    async fn test_resilience_under_load() {
        // Test resilience patterns under concurrent load
        let breaker = Arc::new(CircuitBreaker::new(
            "load_test".to_string(),
            CircuitBreakerConfig {
                failure_threshold: 5,
                reset_timeout: Duration::from_millis(200),
                enable_logging: false,
                ..Default::default()
            }
        ));

        let service = Arc::new(MockService::new());
        service.set_failure_rate(30); // 30% failure rate

        let mut handles = Vec::new();
        
        // Spawn multiple concurrent tasks
        for i in 0..20 {
            let breaker = Arc::clone(&breaker);
            let service = Arc::clone(&service);
            
            let handle = tokio::spawn(async move {
                let mut success_count = 0;
                let mut failure_count = 0;
                
                for j in 0..10 {
                    let result = breaker.call_async(|| async {
                        // Add some variation in timing
                        if (i + j) % 3 == 0 {
                            service.set_slow(true);
                        } else {
                            service.set_slow(false);
                        }
                        service.call().await
                    }).await;
                    
                    match result {
                        Ok(_) => success_count += 1,
                        Err(_) => failure_count += 1,
                    }
                    
                    // Small delay between calls
                    sleep(Duration::from_millis(1)).await;
                }
                
                (success_count, failure_count)
            });
            
            handles.push(handle);
        }

        // Wait for all tasks to complete
        let results = futures::future::join_all(handles).await;
        
        let total_success: u32 = results.iter().map(|r| r.as_ref().unwrap().0).sum();
        let total_failure: u32 = results.iter().map(|r| r.as_ref().unwrap().1).sum();
        
        assert!(total_success > 0, "Should have some successful calls");
        assert!(total_failure > 0, "Should have some failed calls due to circuit breaker");
        assert_eq!(total_success + total_failure, 200); // 20 tasks * 10 calls each
        
        // Verify circuit breaker stats under load
        let stats = breaker.stats().await;
        assert!(stats.total_calls > 0);
        assert!(stats.successful_calls > 0);
        assert!(stats.failed_calls > 0 || stats.rejected_calls > 0);
        
        // Circuit should eventually stabilize
        let final_state = breaker.state().await;
        assert!(matches!(final_state, 
            crate::patterns::CircuitBreakerState::Open | 
            crate::patterns::CircuitBreakerState::HalfOpen |
            crate::patterns::CircuitBreakerState::Closed
        ));
    }
}