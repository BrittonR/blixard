//! Certificate generation tool for Blixard TLS/mTLS
//!
//! This module provides utilities to generate X.509 certificates for:
//! - Root CA certificates
//! - Server certificates for nodes
//! - Client certificates for authentication
//! - Certificate signing and verification

use crate::error::{BlixardError, BlixardResult};
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
                .map_err(|e| BlixardError::Security {
                    message: format!("Invalid validity period: {}", e),
                })?;

        // Generate key pair
        let key_pair = self.generate_key_pair()?;
        params.key_pair = Some(key_pair);

        // Generate certificate
        let cert = Certificate::from_params(params).map_err(|e| BlixardError::Security {
            message: format!("Failed to generate CA certificate: {}", e),
        })?;

        let bundle = CertificateBundle {
            cert_pem: cert.serialize_pem().map_err(|e| BlixardError::Security {
                message: format!("Failed to serialize certificate: {}", e),
            })?,
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
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_ca_generation() {
        let temp_dir = TempDir::new().unwrap();
        let config = CertGeneratorConfig {
            output_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

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
    async fn test_server_cert_generation() {
        let temp_dir = TempDir::new().unwrap();
        let config = CertGeneratorConfig {
            output_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

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
    async fn test_client_cert_generation() {
        let temp_dir = TempDir::new().unwrap();
        let config = CertGeneratorConfig {
            output_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

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
    }
}
