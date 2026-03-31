use anyhow::{Context, Result, bail};
use instant_acme::{
    Account, AccountCredentials, CertificateIdentifier, ChallengeType, Identifier, LetsEncrypt,
    NewAccount, NewOrder, OrderStatus,
};
use rcgen::{CertificateParams, DistinguishedName, KeyPair};
use std::net::IpAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const ACCOUNT_KEY_PATH: &str = "account.json";
const CERT_PATH: &str = "cert.pem";
const KEY_PATH: &str = "key.pem";

pub enum AcmeAction {
    UsedCache,
    Renewed(PathBuf, PathBuf),
}

#[derive(Clone)]
pub struct AcmeManager {
    cert_file: PathBuf,
    key_file: PathBuf,
    account_file: PathBuf,
}

impl AcmeManager {
    pub async fn new() -> Result<Self> {
        let name = env!("CARGO_PKG_NAME");
        let home_dir = std::env::var("HOME").context("Failed to get HOME directory")?;
        let acme_dir = PathBuf::from(format!("{}/.{}/acme", home_dir, name));

        if !acme_dir.exists() {
            fs::create_dir_all(&acme_dir).await?;
        }

        let cert_file = acme_dir.join(CERT_PATH);
        let key_file = acme_dir.join(KEY_PATH);
        let account_file = acme_dir.join(ACCOUNT_KEY_PATH);

        Ok(Self {
            cert_file,
            key_file,
            account_file,
        })
    }

    pub async fn ensure_ip_certificate(&self) -> Result<AcmeAction> {
        // Check existing certificate and ARI
        let mut needs_renewal = true;
        let mut existing_cert_id_string = None;

        if self.cert_file.exists() && self.key_file.exists() {
            let pem_bytes = fs::read(&self.cert_file).await?;
            if let Some(der) = extract_first_cert_der(&pem_bytes) {
                let cert_der = rustls::pki_types::CertificateDer::from(der);
                match CertificateIdentifier::try_from(&cert_der) {
                    Ok(cert_id) => {
                        existing_cert_id_string = Some(format!(
                            "{}.{}",
                            cert_id.authority_key_identifier, cert_id.serial
                        ));
                        tracing::info!("Found existing certificate. Will check ARI window...");
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse existing certificate for ARI: {}", e);
                    }
                }
            } else {
                tracing::warn!("Existing certificate found but could not parse DER");
            }
        }

        // Initialize or load Account
        let builder =
            Account::builder().context("Failed to create Let's Encrypt account builder")?;

        let account = if self.account_file.exists() {
            tracing::info!("Loading existing Let's Encrypt account...");
            let creds_json = fs::read_to_string(&self.account_file).await?;
            let creds: AccountCredentials =
                serde_json::from_str(&creds_json).context("Failed to parse account credentials")?;
            builder
                .from_credentials(creds)
                .await
                .context("Failed to recover account from credentials")?
        } else {
            tracing::info!("Creating new Let's Encrypt account...");
            let new_account = NewAccount {
                contact: &[], // No contact email for now, can be changed later
                terms_of_service_agreed: true,
                only_return_existing: false,
            };
            let (acc, creds) = builder
                .create(
                    &new_account,
                    LetsEncrypt::Production.url().to_string(),
                    None,
                )
                .await
                .context("Failed to create Let's Encrypt account")?;

            let creds_json = serde_json::to_string(&creds)?;
            fs::write(&self.account_file, creds_json).await?;
            acc
        };

        if let Some(cert_id_str) = &existing_cert_id_string
            && let Some((aki, serial)) = cert_id_str.split_once('.')
        {
            let cert_id = CertificateIdentifier {
                authority_key_identifier: std::borrow::Cow::Borrowed(aki),
                serial: std::borrow::Cow::Borrowed(serial),
            };
            tracing::info!("Checking ARI for existing certificate...");
            match account.renewal_info(&cert_id).await {
                Ok(ari) => {
                    let now = time::OffsetDateTime::now_utc();
                    if ari.0.suggested_window.start <= now {
                        tracing::info!("ARI suggests renewal. Proceeding with renewal.");
                        needs_renewal = true;
                    } else {
                        tracing::info!(
                            "ARI says certificate is still valid and not in renewal window."
                        );
                        needs_renewal = false;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to get ARI info: {}. Will assume renewal is needed.",
                        e
                    );
                    needs_renewal = true;
                }
            }
        }

        if !needs_renewal {
            tracing::info!(
                "Using cached Let's Encrypt certificate at {:?}",
                self.cert_file
            );
            return Ok(AcmeAction::UsedCache);
        }

        // Get public IP only when we actually need to order a certificate
        tracing::info!("Fetching public IP address from api.ipify.org...");
        let ip_str = reqwest::get("https://api.ipify.org").await?.text().await?;
        let public_ip = IpAddr::from_str(ip_str.trim()).context("Invalid IP address from ipify")?;
        tracing::info!("Public IP: {}", public_ip);

        tracing::info!("Starting Let's Encrypt order for IP {}...", public_ip);

        let identifier = Identifier::Ip(public_ip);
        let id_slice = &[identifier];
        let new_order = NewOrder::new(id_slice).profile("shortlived");

        let mut order = account
            .new_order(&new_order)
            .await
            .context("Failed to create new order")?;

        tracing::info!("Fetching authorizations...");
        let mut authzs = order.authorizations();

        struct AbortGuard(Vec<tokio::task::JoinHandle<()>>);
        impl Drop for AbortGuard {
            fn drop(&mut self) {
                for task in &self.0 {
                    task.abort();
                }
            }
        }
        let mut server_tasks = AbortGuard(Vec::new());

        while let Some(authz) = authzs.next().await {
            let mut authz = authz.context("Failed to fetch authorization")?;

            if let Some(mut challenge) = authz.challenge(ChallengeType::Http01) {
                let token = challenge.token.clone();
                let key_auth = challenge.key_authorization().as_str().to_string();

                let path = format!("/.well-known/acme-challenge/{}", token);
                tracing::info!(
                    "Starting HTTP challenge server on port 80. Expecting request on {}",
                    path
                );

                let listener = TcpListener::bind("0.0.0.0:80").await.context(
                    "Failed to bind to port 80 for ACME challenge. Are you running as root?",
                )?;

                let path_clone = path.clone();
                let key_auth_clone = key_auth.clone();
                let server_handle = tokio::spawn(async move {
                    loop {
                        match listener.accept().await {
                            Ok((mut stream, _)) => {
                                let mut buf = [0; 1024];
                                if let Ok(n) = stream.read(&mut buf).await {
                                    let req = String::from_utf8_lossy(&buf[..n]);
                                    if req.contains(&path_clone) {
                                        tracing::info!("Received ACME challenge request!");
                                        let response = format!(
                                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{}",
                                            key_auth_clone.len(),
                                            key_auth_clone
                                        );
                                        let _ = stream.write_all(response.as_bytes()).await;
                                    } else {
                                        tracing::warn!(
                                            "Received unexpected HTTP request: \n{}",
                                            req
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("Failed to accept connection on port 80: {}", e);
                            }
                        }
                    }
                });
                server_tasks.0.push(server_handle);

                challenge
                    .set_ready()
                    .await
                    .context("Failed to set challenge ready")?;
            } else {
                tracing::warn!("No HTTP-01 challenge found in authorization");
            }
        }

        tracing::info!("Waiting for order to be ready...");
        let state = order
            .poll_ready(&instant_acme::RetryPolicy::new())
            .await
            .context("Failed while waiting for order to be ready")?;

        drop(server_tasks); // Explicitly abort the background challenge servers

        if let OrderStatus::Ready = state {
            tracing::info!("Order ready! Generating CSR and finalizing...");

            let mut params = CertificateParams::new(vec![public_ip.to_string()])?;
            params.distinguished_name = DistinguishedName::new();
            let key_pair = KeyPair::generate().context("Failed to generate key pair")?;
            let csr = params
                .serialize_request(&key_pair)
                .context("Failed to generate CSR")?;

            order
                .finalize_csr(csr.der())
                .await
                .context("Failed to finalize order")?;

            tracing::info!("Waiting for certificate...");
            let cert_chain_pem = order
                .certificate()
                .await?
                .context("Certificate not found after finalization")?;

            fs::write(&self.cert_file, cert_chain_pem).await?;
            fs::write(&self.key_file, key_pair.serialize_pem()).await?;

            tracing::info!("Successfully obtained and saved Let's Encrypt certificate!");
        } else {
            bail!("Order did not reach ready state: {:?}", state);
        }

        Ok(AcmeAction::Renewed(
            self.cert_file.clone(),
            self.key_file.clone(),
        ))
    }

    pub fn start_background_renewal<F>(&self, mut on_renew: F)
    where
        F: FnMut((PathBuf, PathBuf)) + Send + 'static,
    {
        let manager = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(3 * 3600)).await;
                tracing::info!("Running periodic Let's Encrypt certificate check...");
                match manager.ensure_ip_certificate().await {
                    Ok(AcmeAction::Renewed(cert_path, key_path)) => {
                        tracing::info!("Certificate renewed, reloading...");
                        on_renew((cert_path, key_path));
                    }
                    Ok(AcmeAction::UsedCache) => {
                        tracing::debug!("Periodic check completed. No renewal necessary.");
                    }
                    Err(e) => {
                        let err_msg = format!("{e:#}");
                        tracing::error!(
                            "Failed to generate/load Let's Encrypt certificate during periodic check: {}",
                            err_msg
                        );
                    }
                }
            }
        });
    }
}

fn extract_first_cert_der(pem_bytes: &[u8]) -> Option<Vec<u8>> {
    let pems = rustls_pemfile::certs(&mut std::io::Cursor::new(pem_bytes))
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    pems.into_iter().next().map(|cert| cert.to_vec())
}
