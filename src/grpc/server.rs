use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::encoder::InvoiceEncoder;
use crate::grpc::service::HoldService;
use crate::grpc::service::hold::hold_server::HoldServer;
use crate::grpc::tls::load_certificates;
use crate::messenger::Messenger;
use crate::settler::Settler;
use anyhow::Result;
use log::info;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;
use tokio_util::sync::CancellationToken;
use tonic::transport::ServerTlsConfig;

pub struct State<T, E> {
    pub our_id: [u8; 33],
    pub encoder: E,
    pub invoice_helper: T,
    pub settler: Settler<T>,
    pub messenger: Messenger,
}

pub struct Server<T, E> {
    host: String,
    port: i64,
    is_regtest: bool,

    directory: PathBuf,
    cancellation_token: CancellationToken,

    state: State<T, E>,
}

impl<T, E> Server<T, E>
where
    T: InvoiceHelper + Sync + Send + Clone + 'static,
    E: InvoiceEncoder + Sync + Send + Clone + 'static,
{
    pub fn new(
        host: &str,
        port: i64,
        is_regtest: bool,
        cancellation_token: CancellationToken,
        directory: PathBuf,
        state: State<T, E>,
    ) -> Self {
        Self {
            port,
            state,
            directory,
            is_regtest,
            cancellation_token,
            host: host.to_string(),
        }
    }

    pub async fn start(&self) -> Result<()> {
        if self.port == -1 {
            info!("Not starting gRPC server");
            let token = self.cancellation_token.clone();
            tokio::spawn(async move {
                token.cancelled().await;
            })
            .await?;
            return Ok(());
        }

        // Always listen to all interfaces on regtest
        let socket_addr = SocketAddr::new(
            IpAddr::from_str(if !self.is_regtest {
                self.host.as_str()
            } else {
                "0.0.0.0"
            })?,
            self.port as u16,
        );
        info!("Starting gRPC server on: {}", socket_addr);

        let (identity, ca) = load_certificates(self.directory.clone())?;
        let mut server = tonic::transport::Server::builder().tls_config(
            ServerTlsConfig::new()
                .identity(identity)
                .client_ca_root(ca)
                .client_auth_optional(false),
        )?;

        Ok(server
            .add_service(HoldServer::new(HoldService::new(
                self.state.our_id,
                self.state.invoice_helper.clone(),
                self.state.encoder.clone(),
                self.state.settler.clone(),
                self.state.messenger.clone(),
            )))
            .serve_with_shutdown(socket_addr, async move {
                self.cancellation_token.cancelled().await;
                info!("Shutting down gRPC server");
            })
            .await?)
    }
}

#[cfg(test)]
mod test {
    use crate::database::helpers::invoice_helper::InvoiceHelper;
    use crate::database::model::*;
    use crate::encoder::{InvoiceBuilder, InvoiceEncoder};
    use crate::grpc::server::{Server, State};
    use crate::grpc::service::hold::GetInfoRequest;
    use crate::grpc::service::hold::hold_client::HoldClient;
    use crate::messenger::Messenger;
    use crate::settler::Settler;
    use anyhow::Result;
    use mockall::mock;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::Duration;
    use tokio::task::JoinHandle;
    use tokio_util::sync::CancellationToken;
    use tonic::async_trait;
    use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};

    mock! {
        InvoiceHelper {}

        impl Clone for InvoiceHelper {
            fn clone(&self) -> Self;
        }

        impl InvoiceHelper for InvoiceHelper {
            fn insert(&self, invoice: &InvoiceInsertable) -> Result<usize>;
            fn insert_htlc(&self, htlc: &HtlcInsertable) -> Result<usize>;

            fn set_invoice_state(
                &self,
                id: i64,
                state: InvoiceState,
                new_state: InvoiceState,
            ) -> Result<usize>;
            fn set_invoice_preimage(&self, id: i64, preimage: &[u8]) -> Result<usize>;
            fn set_htlc_state_by_id(
                &self,
                htlc_id: i64,
                state: InvoiceState,
                new_state: InvoiceState,
            ) -> Result<usize>;
            fn set_htlc_states_by_invoice(
                &self,
                invoice_id: i64,
                state: InvoiceState,
                new_state: InvoiceState,
            ) -> Result<usize>;

            fn clean_cancelled(&self, age: Option<u64>) -> Result<usize>;

            fn get_all(&self) -> Result<Vec<HoldInvoice>>;
            fn get_paginated(&self, index_start: i64, limit: u64) -> Result<Vec<HoldInvoice>>;
            fn get_by_payment_hash(&self, payment_hash: &[u8]) -> Result<Option<HoldInvoice>>;
        }
    }

    mock! {
        InvoiceEncoder {}

        impl Clone for InvoiceEncoder {
            fn clone(&self) -> Self;
        }

        #[async_trait]
        impl InvoiceEncoder for InvoiceEncoder {
            async fn encode(&self, invoice_builder: InvoiceBuilder) -> Result<String>;
        }
    }

    #[tokio::test]
    async fn connect() {
        let port = 9124;
        let (certs_dir, token, server_thread) = start_server_tls(port).await;

        let tls = ClientTlsConfig::new()
            .domain_name("hold")
            .ca_certificate(Certificate::from_pem(
                fs::read_to_string(certs_dir.clone().join("ca.pem")).unwrap(),
            ))
            .identity(Identity::from_pem(
                fs::read_to_string(certs_dir.clone().join("client.pem")).unwrap(),
                fs::read_to_string(certs_dir.clone().join("client-key.pem")).unwrap(),
            ));

        let channel = Channel::from_shared(format!("https://127.0.0.1:{}", port))
            .unwrap()
            .tls_config(tls)
            .unwrap()
            .connect()
            .await
            .unwrap();

        let mut client = HoldClient::new(channel);

        let res = client.get_info(GetInfoRequest {}).await.unwrap();
        assert_eq!(
            res.into_inner().version,
            crate::utils::built_info::PKG_VERSION
        );

        token.cancel();
        server_thread.await.unwrap();

        fs::remove_dir_all(certs_dir).unwrap()
    }

    #[tokio::test]
    async fn connect_invalid_client_certificate() {
        let port = 9125;
        let (certs_dir, token, server_thread) = start_server_tls(port).await;

        let tls = ClientTlsConfig::new()
            .domain_name("hold")
            .ca_certificate(Certificate::from_pem(
                fs::read_to_string(certs_dir.clone().join("ca.pem")).unwrap(),
            ));

        let channel = Channel::from_shared(format!("https://127.0.0.1:{}", port))
            .unwrap()
            .tls_config(tls)
            .unwrap()
            .connect()
            .await
            .unwrap();

        let mut client = HoldClient::new(channel);

        let res = client.get_info(GetInfoRequest {}).await;
        assert_eq!(res.err().unwrap().message(), "transport error");

        token.cancel();
        server_thread.await.unwrap();

        fs::remove_dir_all(certs_dir).unwrap()
    }

    async fn start_server_tls(port: i64) -> (PathBuf, CancellationToken, JoinHandle<()>) {
        let certs_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("test-certs-{}", port));

        let token = CancellationToken::new();
        let server = Server::new(
            "127.0.0.1",
            port,
            false,
            token.clone(),
            certs_dir.clone(),
            State {
                our_id: [0; 33],
                messenger: Messenger::new(),
                encoder: make_mock_invoice_encoder(),
                invoice_helper: make_mock_invoice_helper(),
                settler: Settler::new(make_mock_invoice_helper(), 60),
            },
        );

        let server_thread = tokio::spawn(async move {
            server.start().await.unwrap();
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        (certs_dir, token, server_thread)
    }

    fn make_mock_invoice_helper() -> MockInvoiceHelper {
        let mut hook_helper = MockInvoiceHelper::new();
        hook_helper
            .expect_clone()
            .returning(make_mock_invoice_helper);

        hook_helper
    }

    fn make_mock_invoice_encoder() -> MockInvoiceEncoder {
        let mut invoice_encoder = MockInvoiceEncoder::new();
        invoice_encoder
            .expect_clone()
            .returning(make_mock_invoice_encoder);

        invoice_encoder
    }
}
