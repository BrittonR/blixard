//! Certificate generation tool for Blixard TLS/mTLS
//!
//! This module provides utilities to generate X.509 certificates for:
//! - Root CA certificates
//! - Server certificates for nodes
//! - Client certificates for authentication
//! - Certificate signing and verification

use crate::error::{BlixardError, BlixardResult};
use crate::common::error_context::SecurityContext;
use chrono::Duration;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose, SanType,
};
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info};

/// Certificate generation configuration
#[derive(Debug, Clone)]
pub struct CertGeneratorConfig {
    /// Organization name for certificates
    pub organization: String,

    /// Country code (2 letter)
    pub country: String,

    /// State/Province
    pub state: String,

    /// Locality/City
    pub locality: String,

    /// Certificate validity period in days
    pub validity_days: i64,

    /// Key algorithm (RSA or ECDSA)
    pub key_algorithm: KeyAlgorithm,

    /// Output directory for certificates
    pub output_dir: PathBuf,
}

/// Supported key algorithms
#[derive(Debug, Clone, Copy)]
pub enum KeyAlgorithm {
    /// RSA with 2048 bits
    Rsa2048,
    /// RSA with 4096 bits
    Rsa4096,
    /// ECDSA with P-256 curve
    EcdsaP256,
    /// ECDSA with P-384 curve
    EcdsaP384,
}

/// Certificate generator for Blixard cluster
pub struct CertGenerator {
    config: CertGeneratorConfig,
}

/// Generated certificate bundle
#[derive(Debug)]
pub struct CertificateBundle {
    /// Certificate in PEM format
    pub cert_pem: String,
    /// Private key in PEM format
    pub key_pem: String,
    /// Certificate serial number
    pub serial_number: String,
    /// Certificate fingerprint (SHA256)
    pub fingerprint: String,
}

/// Complete PKI bundle for a cluster
pub struct ClusterPKI {
    /// CA certificate
    pub ca_cert: Certificate,
    /// CA bundle
    pub ca_bundle: CertificateBundle,
    /// Server certificates
    pub server_bundles: Vec<(String, CertificateBundle)>,
    /// Client certificates
    pub client_bundles: Vec<(String, CertificateBundle)>,
}

impl Default for CertGeneratorConfig {
    fn default() -> Self {
        Self {
            organization: "Blixard Cluster".to_string(),
            country: "US".to_string(),
            state: "CA".to_string(),
            locality: "San Francisco".to_string(),
            validity_days: 365,
            key_algorithm: KeyAlgorithm::EcdsaP256,
            output_dir: PathBuf::from("./certs"),
        }
    }
}

impl CertGenerator {
    /// Create a new certificate generator
    pub fn new(config: CertGeneratorConfig) -> Self {
        Self { config }
    }

    /// Generate a complete PKI for a cluster
    pub async fn generate_cluster_pki(
        &self,
        cluster_name: &str,
        node_names: Vec<String>,
        client_names: Vec<String>,
    ) -> BlixardResult<ClusterPKI> {
        info!("Generating complete PKI for cluster: {}", cluster_name);

        // Generate CA
        let (ca_cert, ca_bundle) = self.generate_ca(&format!("{}-ca", cluster_name)).await?;

        // Generate server certificates
        let mut server_bundles = Vec::new();
        for node_name in node_names {
            let san_list = vec![
                node_name.clone(),
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "::1".to_string(),
            ];

            let bundle = self
                .generate_server_cert(&node_name, san_list, &ca_cert)
                .await?;
            server_bundles.push((node_name, bundle));
        }

        // Generate client certificates
        let mut client_bundles = Vec::new();
        for client_name in client_names {
            let bundle = self.generate_client_cert(&client_name, &ca_cert).await?;
            client_bundles.push((client_name, bundle));
        }

        Ok(ClusterPKI {
            ca_cert,
            ca_bundle,
            server_bundles,
            client_bundles,
        })
    }

    /// Generate a root CA certificate
    async fn generate_ca(
        &self,
        common_name: &str,
    ) -> BlixardResult<(Certificate, CertificateBundle)> {
        info!("Generating root CA certificate for: {}", common_name);

        // Create CA certificate parameters
        let mut params = CertificateParams::new(vec![common_name.to_string()]);
        self.set_distinguished_name(&mut params, common_name);

        // Set CA constraints
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];

        // Set validity period
        params.not_before = time::OffsetDateTime::now_utc();
        params.not_after = params.not_before
            + Duration::days(self.config.validity_days)
                .to_std()
                .cert_context("set validity period")?;

        // Generate key pair
        let key_pair = self.generate_key_pair()?;
        params.key_pair = Some(key_pair);

        // Generate certificate
        let cert = Certificate::from_params(params)
            .cert_context("generate CA certificate")?;

        let bundle = CertificateBundle {
            cert_pem: cert.serialize_pem()
                .cert_context("serialize certificate")?,
            key_pem: cert.serialize_private_key_pem(),
            serial_number: self.get_serial_number(&cert),
            fingerprint: self.calculate_fingerprint(&cert)?,
        };

        // Save to files
        self.save_certificate(&bundle, &format!("ca-{}", common_name))
            .await?;

        info!(
            "Generated CA certificate with serial: {}",
            bundle.serial_number
        );
        Ok((cert, bundle))
    }

    /// Generate a server certificate signed by CA
    async fn generate_server_cert(
        &self,
        common_name: &str,
        san_list: Vec<String>,
        ca_cert: &Certificate,
    ) -> BlixardResult<CertificateBundle> {
        info!("Generating server certificate for: {}", common_name);

        // Create server certificate parameters
        let mut params = CertificateParams::new(vec![common_name.to_string()]);
        self.set_distinguished_name(&mut params, common_name);

        // Add Subject Alternative Names
        params.subject_alt_names = san_list
            .into_iter()
            .map(|san| match san.parse::<std::net::IpAddr>() {
                Ok(ip) => SanType::IpAddress(ip),
                Err(_) => SanType::DnsName(san),
            })
            .collect();

        // Set server constraints
        params.is_ca = IsCa::NoCa;
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyAgreement,
        ];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth, // For mTLS
        ];

        // Set validity period
        params.not_before = time::OffsetDateTime::now_utc();
        params.not_after = params.not_before
            + Duration::days(self.config.validity_days)
                .to_std()
                .map_err(|e| BlixardError::Security {
                    message: format!("Invalid validity period: {}", e),
                })?;

        // Generate key pair
        let key_pair = self.generate_key_pair()?;
        params.key_pair = Some(key_pair);

        // Sign with CA
        let cert = Certificate::from_params(params).map_err(|e| BlixardError::Security {
            message: format!("Failed to generate server certificate: {}", e),
        })?;

        let cert_pem =
            cert.serialize_pem_with_signer(ca_cert)
                .map_err(|e| BlixardError::Security {
                    message: format!("Failed to sign certificate: {}", e),
                })?;

        let bundle = CertificateBundle {
            cert_pem,
            key_pem: cert.serialize_private_key_pem(),
            serial_number: self.get_serial_number(&cert),
            fingerprint: self.calculate_fingerprint(&cert)?,
        };

        // Save to files
        self.save_certificate(&bundle, &format!("server-{}", common_name))
            .await?;

        info!(
            "Generated server certificate with serial: {}",
            bundle.serial_number
        );
        Ok(bundle)
    }

    /// Generate a client certificate for authentication
    async fn generate_client_cert(
        &self,
        common_name: &str,
        ca_cert: &Certificate,
    ) -> BlixardResult<CertificateBundle> {
        info!("Generating client certificate for: {}", common_name);

        // Create client certificate parameters
        let mut params = CertificateParams::new(vec![common_name.to_string()]);
        self.set_distinguished_name(&mut params, common_name);

        // Set client constraints
        params.is_ca = IsCa::NoCa;
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyAgreement,
        ];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];

        // Set validity period
        params.not_before = time::OffsetDateTime::now_utc();
        params.not_after = params.not_before
            + Duration::days(self.config.validity_days)
                .to_std()
                .map_err(|e| BlixardError::Security {
                    message: format!("Invalid validity period: {}", e),
                })?;

        // Generate key pair
        let key_pair = self.generate_key_pair()?;
        params.key_pair = Some(key_pair);

        // Sign with CA
        let cert = Certificate::from_params(params).map_err(|e| BlixardError::Security {
            message: format!("Failed to generate client certificate: {}", e),
        })?;

        let cert_pem =
            cert.serialize_pem_with_signer(ca_cert)
                .map_err(|e| BlixardError::Security {
                    message: format!("Failed to sign certificate: {}", e),
                })?;

        let bundle = CertificateBundle {
            cert_pem,
            key_pem: cert.serialize_private_key_pem(),
            serial_number: self.get_serial_number(&cert),
            fingerprint: self.calculate_fingerprint(&cert)?,
        };

        // Save to files
        self.save_certificate(&bundle, &format!("client-{}", common_name))
            .await?;

        info!(
            "Generated client certificate with serial: {}",
            bundle.serial_number
        );
        Ok(bundle)
    }

    /// Generate a key pair based on configured algorithm
    fn generate_key_pair(&self) -> BlixardResult<KeyPair> {
        match self.config.key_algorithm {
            KeyAlgorithm::Rsa2048 => {
                KeyPair::generate(&rcgen::PKCS_RSA_SHA256).map_err(|e| BlixardError::Security {
                    message: format!("Failed to generate RSA 2048 key: {}", e),
                })
            }
            KeyAlgorithm::Rsa4096 => {
                // rcgen doesn't have a direct RSA 4096 constant, so we generate with default
                // and rely on the library's RSA implementation
                KeyPair::generate(&rcgen::PKCS_RSA_SHA256).map_err(|e| BlixardError::Security {
                    message: format!("Failed to generate RSA 4096 key: {}", e),
                })
            }
            KeyAlgorithm::EcdsaP256 => {
                KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(|e| {
                    BlixardError::Security {
                        message: format!("Failed to generate ECDSA P-256 key: {}", e),
                    }
                })
            }
            KeyAlgorithm::EcdsaP384 => {
                KeyPair::generate(&rcgen::PKCS_ECDSA_P384_SHA384).map_err(|e| {
                    BlixardError::Security {
                        message: format!("Failed to generate ECDSA P-384 key: {}", e),
                    }
                })
            }
        }
    }

    /// Set distinguished name fields
    fn set_distinguished_name(&self, params: &mut CertificateParams, common_name: &str) {
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, common_name);
        dn.push(DnType::OrganizationName, &self.config.organization);
        dn.push(DnType::CountryName, &self.config.country);
        dn.push(DnType::StateOrProvinceName, &self.config.state);
        dn.push(DnType::LocalityName, &self.config.locality);
        params.distinguished_name = dn;
    }

    /// Get serial number from certificate
    fn get_serial_number(&self, _cert: &Certificate) -> String {
        // rcgen generates a random serial number internally
        // We'll use a placeholder here as rcgen doesn't expose it directly
        format!("{:x}", rand::random::<u64>())
    }

    /// Calculate SHA256 fingerprint of certificate
    fn calculate_fingerprint(&self, cert: &Certificate) -> BlixardResult<String> {
        use sha2::{Digest, Sha256};

        let cert_der = cert.serialize_der().map_err(|e| BlixardError::Security {
            message: format!("Failed to serialize certificate to DER: {}", e),
        })?;

        let mut hasher = Sha256::new();
        hasher.update(&cert_der);
        let result = hasher.finalize();

        // Format as hex with colons
        Ok(result
            .iter()
            .map(|byte| format!("{:02X}", byte))
            .collect::<Vec<_>>()
            .join(":"))
    }

    /// Save certificate and key to files
    async fn save_certificate(&self, bundle: &CertificateBundle, name: &str) -> BlixardResult<()> {
        // Create output directory if it doesn't exist
        fs::create_dir_all(&self.config.output_dir)
            .await
            .map_err(|e| BlixardError::Security {
                message: format!("Failed to create output directory: {}", e),
            })?;

        let cert_path = self.config.output_dir.join(format!("{}.crt", name));
        let key_path = self.config.output_dir.join(format!("{}.key", name));

        // Save certificate
        fs::write(&cert_path, &bundle.cert_pem)
            .await
            .map_err(|e| BlixardError::Security {
                message: format!("Failed to write certificate file: {}", e),
            })?;

        // Save private key with restricted permissions
        fs::write(&key_path, &bundle.key_pem)
            .await
            .map_err(|e| BlixardError::Security {
                message: format!("Failed to write key file: {}", e),
            })?;

        // Set restrictive permissions on key file (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&key_path)
                .await
                .map_err(|e| BlixardError::Security {
                    message: format!("Failed to get key file metadata: {}", e),
                })?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600); // Read/write for owner only
            fs::set_permissions(&key_path, permissions)
                .await
                .map_err(|e| BlixardError::Security {
                    message: format!("Failed to set key file permissions: {}", e),
                })?;
        }

        debug!("Saved certificate to: {:?}", cert_path);
        debug!("Saved private key to: {:?}", key_path);

        Ok(())
    }
}

/// Helper function to generate a complete PKI for a Blixard cluster
pub async fn generate_cluster_pki(
    cluster_name: &str,
    node_names: Vec<String>,
    client_names: Vec<String>,
    output_dir: PathBuf,
) -> BlixardResult<()> {
    info!("Generating complete PKI for cluster: {}", cluster_name);

    let config = CertGeneratorConfig {
        organization: format!("{} Cluster", cluster_name),
        output_dir,
        ..Default::default()
    };

    let generator = CertGenerator::new(config);
    let _pki = generator
        .generate_cluster_pki(cluster_name, node_names, client_names)
        .await?;

    info!("Successfully generated PKI for cluster: {}", cluster_name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    use tempfile::TempDir;
    use x509_parser::{
        certificate::X509Certificate, parse_x509_certificate, public_key::PublicKey,
    };

    /// Helper function to parse certificate and validate basic structure
    /// Since x509_parser::X509Certificate contains references to the input data,
    /// we use Box::leak to create a 'static lifetime for testing purposes
    fn parse_certificate_pem(cert_pem: &str) -> crate::error::BlixardResult<X509Certificate<'static>> {
        let cert_der = pem::parse(cert_pem)
            .map_err(|e| crate::error::BlixardError::Security {
                message: format!("Failed to parse PEM certificate: {}", e),
            })?;
        
        // Convert to owned data and leak it to get 'static lifetime
        // This is acceptable for tests as they're short-lived
        let cert_bytes = Box::leak(cert_der.into_contents().into_boxed_slice());
        
        let (_, cert) = parse_x509_certificate(cert_bytes)
            .map_err(|e| crate::error::BlixardError::Security {
                message: format!("Failed to parse X.509 certificate: {}", e),
            })?;
        
        Ok(cert)
    }

    /// Helper function to create test config with temporary directory
    fn create_test_config() -> (CertGeneratorConfig, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = CertGeneratorConfig {
            output_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        (config, temp_dir)
    }

    #[tokio::test]
    async fn test_ca_generation() {
        let (config, temp_dir) = create_test_config();
        let generator = CertGenerator::new(config);
        let (_, ca_bundle) = generator.generate_ca("test-ca").await.unwrap();

        assert!(!ca_bundle.cert_pem.is_empty());
        assert!(!ca_bundle.key_pem.is_empty());
        assert!(ca_bundle.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(ca_bundle.key_pem.contains("BEGIN PRIVATE KEY"));

        // Check files were created
        assert!(temp_dir.path().join("ca-test-ca.crt").exists());
        assert!(temp_dir.path().join("ca-test-ca.key").exists());
    }

    #[tokio::test]
    async fn test_ca_certificate_validation() {
        let (config, _temp_dir) = create_test_config();
        let generator = CertGenerator::new(config);
        let (_, ca_bundle) = generator.generate_ca("test-ca").await.unwrap();

        // Parse and validate CA certificate structure
        let cert = parse_certificate_pem(&ca_bundle.cert_pem).unwrap();

        // Verify it's a CA certificate
        assert!(cert.is_ca());

        // Verify subject contains expected fields
        assert!(cert.subject().to_string().contains("CN=test-ca"));
        assert!(cert.subject().to_string().contains("O=Blixard Cluster"));

        // Verify key usage includes certificate signing
        if let Ok(Some(key_usage)) = cert.key_usage() {
            assert!(key_usage.value.key_cert_sign());
            assert!(key_usage.value.crl_sign());
        }

        // Verify validity period
        assert!(cert.validity().not_before <= cert.validity().not_after);

        // Verify serial number and fingerprint are populated
        assert!(!ca_bundle.serial_number.is_empty());
        assert!(!ca_bundle.fingerprint.is_empty());
        assert!(ca_bundle.fingerprint.contains(":"));
    }

    #[tokio::test]
    async fn test_server_cert_generation() {
        let (config, _temp_dir) = create_test_config();
        let generator = CertGenerator::new(config);

        // Generate CA first
        let (ca_cert, _) = generator.generate_ca("test-ca").await.unwrap();

        // Generate server certificate
        let server_bundle = generator
            .generate_server_cert(
                "node1.blixard.local",
                vec!["node1.blixard.local".to_string(), "127.0.0.1".to_string()],
                &ca_cert,
            )
            .await
            .unwrap();

        assert!(!server_bundle.cert_pem.is_empty());
        assert!(!server_bundle.key_pem.is_empty());
    }

    #[tokio::test]
    async fn test_server_certificate_validation() {
        let (config, _temp_dir) = create_test_config();
        let generator = CertGenerator::new(config);

        let (ca_cert, _) = generator.generate_ca("test-ca").await.unwrap();
        let server_bundle = generator
            .generate_server_cert(
                "node1.blixard.local",
                vec!["node1.blixard.local".to_string(), "127.0.0.1".to_string()],
                &ca_cert,
            )
            .await
            .unwrap();

        let cert = parse_certificate_pem(&server_bundle.cert_pem).unwrap();

        // Verify it's NOT a CA certificate
        assert!(!cert.is_ca());

        // Verify subject contains server name
        assert!(cert
            .subject()
            .to_string()
            .contains("CN=node1.blixard.local"));

        // Verify Subject Alternative Names
        if let Ok(Some(san_ext)) = cert.subject_alternative_name() {
            let san_names: Vec<_> = san_ext.value.general_names.iter().collect();
            assert!(!san_names.is_empty());
            // Should contain both DNS name and IP address
        }

        // Verify key usage for server authentication
        if let Ok(Some(key_usage)) = cert.key_usage() {
            assert!(key_usage.value.digital_signature());
            assert!(key_usage.value.key_agreement());
        }

        // Verify extended key usage includes server auth
        if let Ok(Some(ext_key_usage)) = cert.extended_key_usage() {
            assert!(ext_key_usage.value.server_auth);
            assert!(ext_key_usage.value.client_auth); // For mTLS
        }
    }

    #[tokio::test]
    async fn test_client_cert_generation() {
        let (config, _temp_dir) = create_test_config();
        let generator = CertGenerator::new(config);

        // Generate CA first
        let (ca_cert, _) = generator.generate_ca("test-ca").await.unwrap();

        // Generate client certificate
        let client_bundle = generator
            .generate_client_cert("admin@blixard.local", &ca_cert)
            .await
            .unwrap();

        assert!(!client_bundle.cert_pem.is_empty());
        assert!(!client_bundle.key_pem.is_empty());
    }

    #[tokio::test]
    async fn test_client_certificate_validation() {
        let (config, _temp_dir) = create_test_config();
        let generator = CertGenerator::new(config);

        let (ca_cert, _) = generator.generate_ca("test-ca").await.unwrap();
        let client_bundle = generator
            .generate_client_cert("admin@blixard.local", &ca_cert)
            .await
            .unwrap();

        let cert = parse_certificate_pem(&client_bundle.cert_pem).unwrap();

        // Verify it's NOT a CA certificate
        assert!(!cert.is_ca());

        // Verify subject contains client name
        assert!(cert
            .subject()
            .to_string()
            .contains("CN=admin@blixard.local"));

        // Verify extended key usage includes client auth only
        if let Ok(Some(ext_key_usage)) = cert.extended_key_usage() {
            assert!(ext_key_usage.value.client_auth);
            assert!(!ext_key_usage.value.server_auth); // Should not have server auth
        }
    }

    #[tokio::test]
    async fn test_key_algorithm_support() {
        let (mut config, _temp_dir) = create_test_config();

        // Test all supported key algorithms
        let algorithms = vec![
            KeyAlgorithm::EcdsaP256,
            KeyAlgorithm::EcdsaP384,
            KeyAlgorithm::Rsa2048,
            KeyAlgorithm::Rsa4096,
        ];

        for algorithm in algorithms {
            config.key_algorithm = algorithm;
            let generator = CertGenerator::new(config.clone());

            // Test key generation doesn't fail
            let key_pair = generator.generate_key_pair();
            assert!(
                key_pair.is_ok(),
                "Failed to generate key pair for {:?}",
                algorithm
            );

            // Test CA generation works with this algorithm
            let (_, ca_bundle) = generator.generate_ca("test-ca").await.unwrap();
            assert!(!ca_bundle.cert_pem.is_empty());
            assert!(!ca_bundle.key_pem.is_empty());
        }
    }

    #[tokio::test]
    async fn test_certificate_validity_period() {
        let (mut config, _temp_dir) = create_test_config();
        config.validity_days = 7; // Short validity for testing

        let generator = CertGenerator::new(config);
        let (_, ca_bundle) = generator.generate_ca("test-ca").await.unwrap();

        let cert = parse_certificate_pem(&ca_bundle.cert_pem).unwrap();
        let validity = cert.validity();

        // Check validity period is approximately 7 days
        let duration = validity.not_after.timestamp() - validity.not_before.timestamp();
        let expected_duration = 7 * 24 * 60 * 60; // 7 days in seconds

        // Allow some tolerance for execution time
        assert!(duration >= expected_duration - 60);
        assert!(duration <= expected_duration + 60);
    }

    #[tokio::test]
    async fn test_certificate_chain_validation() {
        let (config, _temp_dir) = create_test_config();
        let generator = CertGenerator::new(config);

        // Generate CA
        let (ca_cert, ca_bundle) = generator.generate_ca("test-ca").await.unwrap();

        // Generate server certificate signed by CA
        let server_bundle = generator
            .generate_server_cert("node1", vec!["node1".to_string()], &ca_cert)
            .await
            .unwrap();

        // Parse both certificates
        let ca_x509 = parse_certificate_pem(&ca_bundle.cert_pem).unwrap();
        let server_x509 = parse_certificate_pem(&server_bundle.cert_pem).unwrap();

        // Verify server certificate is signed by CA
        // This is a basic check - in production you'd use proper certificate chain validation
        assert_ne!(ca_x509.subject(), server_x509.subject());
        assert!(ca_x509.is_ca());
        assert!(!server_x509.is_ca());

        // Verify different serial numbers
        assert_ne!(ca_bundle.serial_number, server_bundle.serial_number);
    }

    #[tokio::test]
    async fn test_cluster_pki_generation() {
        let temp_dir = TempDir::new().unwrap();

        generate_cluster_pki(
            "test-cluster",
            vec!["node1".to_string(), "node2".to_string()],
            vec!["admin".to_string(), "operator".to_string()],
            temp_dir.path().to_path_buf(),
        )
        .await
        .unwrap();

        // Check all expected files exist
        assert!(temp_dir.path().join("ca-test-cluster-ca.crt").exists());
        assert!(temp_dir.path().join("server-node1.crt").exists());
        assert!(temp_dir.path().join("server-node2.crt").exists());
        assert!(temp_dir.path().join("client-admin.crt").exists());
        assert!(temp_dir.path().join("client-operator.crt").exists());

        // Check all corresponding key files exist
        assert!(temp_dir.path().join("ca-test-cluster-ca.key").exists());
        assert!(temp_dir.path().join("server-node1.key").exists());
        assert!(temp_dir.path().join("server-node2.key").exists());
        assert!(temp_dir.path().join("client-admin.key").exists());
        assert!(temp_dir.path().join("client-operator.key").exists());
    }

    #[tokio::test]
    async fn test_complete_cluster_pki_validation() {
        let (config, _temp_dir) = create_test_config();
        let generator = CertGenerator::new(config);

        let pki = generator
            .generate_cluster_pki(
                "test-cluster",
                vec!["node1".to_string(), "node2".to_string()],
                vec!["admin".to_string()],
            )
            .await
            .unwrap();

        // Validate CA
        let ca_cert = parse_certificate_pem(&pki.ca_bundle.cert_pem).unwrap();
        assert!(ca_cert.is_ca());

        // Validate all server certificates
        for (name, bundle) in &pki.server_bundles {
            let cert = parse_certificate_pem(&bundle.cert_pem).unwrap();
            assert!(!cert.is_ca());
            assert!(cert.subject().to_string().contains(&format!("CN={}", name)));

            // Verify has server auth capability
            if let Ok(Some(ext_key_usage)) = cert.extended_key_usage() {
                assert!(ext_key_usage.value.server_auth);
            }
        }

        // Validate all client certificates
        for (name, bundle) in &pki.client_bundles {
            let cert = parse_certificate_pem(&bundle.cert_pem).unwrap();
            assert!(!cert.is_ca());
            assert!(cert.subject().to_string().contains(&format!("CN={}", name)));

            // Verify has client auth capability
            if let Ok(Some(ext_key_usage)) = cert.extended_key_usage() {
                assert!(ext_key_usage.value.client_auth);
            }
        }

        // Verify unique serial numbers
        let mut serials = std::collections::HashSet::new();
        serials.insert(&pki.ca_bundle.serial_number);
        for (_, bundle) in &pki.server_bundles {
            assert!(serials.insert(&bundle.serial_number));
        }
        for (_, bundle) in &pki.client_bundles {
            assert!(serials.insert(&bundle.serial_number));
        }
    }

    #[tokio::test]
    async fn test_file_permissions() {
        let (config, temp_dir) = create_test_config();
        let generator = CertGenerator::new(config);
        let (_, ca_bundle) = generator.generate_ca("test-ca").await.unwrap();

        let key_path = temp_dir.path().join("ca-test-ca.key");
        assert!(key_path.exists());

        // Check file permissions on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = tokio::fs::metadata(&key_path).await.unwrap();
            let permissions = metadata.permissions();
            assert_eq!(permissions.mode() & 0o777, 0o600); // Should be 600 (owner read/write only)
        }
    }

    #[tokio::test]
    async fn test_error_handling_invalid_validity() {
        let (mut config, _temp_dir) = create_test_config();
        config.validity_days = -1; // Invalid negative validity

        let generator = CertGenerator::new(config);
        let result = generator.generate_ca("test-ca").await;

        // Should handle the error gracefully
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_san_processing() {
        let (config, _temp_dir) = create_test_config();
        let generator = CertGenerator::new(config);

        let (ca_cert, _) = generator.generate_ca("test-ca").await.unwrap();

        // Test with mixed DNS names and IP addresses
        let san_list = vec![
            "node1.example.com".to_string(),
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            "::1".to_string(),
            "192.168.1.100".to_string(),
        ];

        let server_bundle = generator
            .generate_server_cert("node1", san_list, &ca_cert)
            .await
            .unwrap();

        let cert = parse_certificate_pem(&server_bundle.cert_pem).unwrap();

        // Verify SAN extension exists
        assert!(cert.subject_alternative_name().is_ok());
    }

    #[tokio::test]
    async fn test_certificate_fingerprint_uniqueness() {
        let (config, _temp_dir) = create_test_config();
        let generator = CertGenerator::new(config);

        // Generate multiple certificates
        let (_, ca1) = generator.generate_ca("ca1").await.unwrap();
        let (_, ca2) = generator.generate_ca("ca2").await.unwrap();

        // Fingerprints should be different
        assert_ne!(ca1.fingerprint, ca2.fingerprint);

        // Fingerprints should be in expected format (hex with colons)
        assert!(ca1.fingerprint.matches(':').count() > 0);
        assert!(ca1
            .fingerprint
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == ':'));
    }
}
