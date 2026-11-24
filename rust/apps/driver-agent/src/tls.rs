use std::{io::BufReader, sync::Arc};
use superlink_domain::ConnectionConfig;
pub fn client(config: &ConnectionConfig) -> Result<Arc<rustls::ClientConfig>, String> {
    superlink_drivers::install_crypto_provider();
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    if !config.ssl_ca_path.is_empty() {
        for cert in rustls_pemfile::certs(&mut BufReader::new(pem(&config.ssl_ca_path)?.as_slice()))
        {
            roots
                .add(cert.map_err(|_| "CA 证书无效")?)
                .map_err(|_| "CA 证书无效")?;
        }
    }
    let builder = rustls::ClientConfig::builder().with_root_certificates(roots);
    let client = if config.ssl_cert_path.is_empty() && config.ssl_key_path.is_empty() {
        builder.with_no_client_auth()
    } else {
        let certs =
            rustls_pemfile::certs(&mut BufReader::new(pem(&config.ssl_cert_path)?.as_slice()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "客户端证书无效")?;
        let key =
            rustls_pemfile::private_key(&mut BufReader::new(pem(&config.ssl_key_path)?.as_slice()))
                .map_err(|_| "客户端私钥无效")?
                .ok_or("客户端私钥缺失")?;
        builder
            .with_client_auth_cert(certs, key)
            .map_err(|_| "客户端证书与私钥无效")?
    };
    Ok(Arc::new(client))
}
pub fn pem(path: &str) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|_| "无法读取 TLS 文件")?;
    if !metadata.is_file() || metadata.len() > 1 << 20 {
        return Err("TLS 文件过大或不是普通文件".into());
    }
    std::fs::read(path).map_err(|_| "无法读取 TLS 文件".into())
}
pub async fn cancelled(cancel: &superlink_drivers::QueryCancel) {
    while !cancel.is_cancelled() {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
pub fn native(config: &ConnectionConfig) -> Result<native_tls::TlsConnector, String> {
    let mut builder = native_tls::TlsConnector::builder();
    if !config.ssl_ca_path.is_empty() {
        builder.add_root_certificate(
            native_tls::Certificate::from_pem(&pem(&config.ssl_ca_path)?)
                .map_err(|_| "TLS CA 无效")?,
        );
    }
    if !config.ssl_cert_path.is_empty() || !config.ssl_key_path.is_empty() {
        builder.identity(
            native_tls::Identity::from_pkcs8(
                &pem(&config.ssl_cert_path)?,
                &pem(&config.ssl_key_path)?,
            )
            .map_err(|_| "客户端证书或 PKCS#8 私钥无效")?,
        );
    }
    builder.build().map_err(|_| "TLS 初始化失败".into())
}
/// Keep the database's DNS identity while TCP is forwarded to a loopback port.
pub struct GaussTls {
    pub connector: opengauss_native_tls::MakeTlsConnector,
    pub server_name: String,
}
impl<S> opengauss::tls::MakeTlsConnect<S> for GaussTls
where
    opengauss_native_tls::MakeTlsConnector: opengauss::tls::MakeTlsConnect<S>,
{
    type Stream =
        <opengauss_native_tls::MakeTlsConnector as opengauss::tls::MakeTlsConnect<S>>::Stream;
    type TlsConnect =
        <opengauss_native_tls::MakeTlsConnector as opengauss::tls::MakeTlsConnect<S>>::TlsConnect;
    type Error =
        <opengauss_native_tls::MakeTlsConnector as opengauss::tls::MakeTlsConnect<S>>::Error;
    fn make_tls_connect(&mut self, domain: &str) -> Result<Self::TlsConnect, Self::Error> {
        self.connector
            .make_tls_connect(if self.server_name.is_empty() {
                domain
            } else {
                &self.server_name
            })
    }
}
