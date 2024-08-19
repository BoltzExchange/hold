use crate::grpc::service::hold::hold_server::HoldServer;
use crate::grpc::service::HoldService;
use crate::grpc::tls::load_certificates;
use anyhow::Result;
use log::info;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;
use tonic::transport::ServerTlsConfig;

pub struct Server {
    host: String,
    port: i64,
    directory: PathBuf,
}

impl Server {
    pub fn new(host: &str, port: i64, directory: PathBuf) -> Self {
        Self {
            port,
            directory,
            host: host.to_string(),
        }
    }

    pub async fn start(&self) -> Result<()> {
        let socket_addr = SocketAddr::new(IpAddr::from_str(self.host.as_str())?, self.port as u16);
        info!("Starting gRPC server on: {}", socket_addr);

        let (identity, ca) = load_certificates(self.directory.clone())?;
        let mut server = tonic::transport::Server::builder().tls_config(
            ServerTlsConfig::new()
                .identity(identity)
                .client_ca_root(ca)
                .client_auth_optional(false),
        )?;

        Ok(server
            .add_service(HoldServer::new(HoldService {}))
            .serve(socket_addr)
            .await?)
    }
}
