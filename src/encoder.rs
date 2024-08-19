use anyhow::Result;
use bitcoin::hashes::{sha256, Hash};
use cln_rpc::model::requests::SigninvoiceRequest;
use cln_rpc::ClnRpc;
use lightning_invoice::{Currency, PaymentSecret, RouteHint};
use secp256k1::rand::Rng;
use secp256k1::{rand, Secp256k1, SecretKey};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const DEFAULT_MIN_FINAL_CLTV_EXPIRY_DELTA: u64 = 80;

#[derive(Debug)]
enum NetworkError {
    InvalidNetwork,
}

impl Display for NetworkError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match *self {
            NetworkError::InvalidNetwork => write!(f, "invalid network"),
        }
    }
}

impl Error for NetworkError {}

pub enum InvoiceDescription {
    Description(String),
    Hash(Vec<u8>),
}

pub struct InvoiceBuilder {
    payment_hash: Vec<u8>,
    payment_secret: Option<Vec<u8>>,
    amount_msat: Option<u64>,
    description: Option<InvoiceDescription>,
    expiry: Option<u64>,
    min_final_cltv_expiry_delta: Option<u64>,
    route_hints: Option<Vec<RouteHint>>,
}

impl InvoiceBuilder {
    pub fn new(payment_hash: &[u8]) -> Self {
        InvoiceBuilder {
            payment_hash: payment_hash.to_vec(),
            payment_secret: None,
            amount_msat: None,
            description: None,
            expiry: None,
            min_final_cltv_expiry_delta: None,
            route_hints: None,
        }
    }

    pub fn payment_secret(mut self, secret: &[u8]) -> Self {
        self.payment_secret = Some(secret.to_vec());
        self
    }

    pub fn amount_msat(mut self, amount: u64) -> Self {
        self.amount_msat = Some(amount);
        self
    }

    pub fn description(mut self, description: InvoiceDescription) -> Self {
        self.description = Some(description);
        self
    }

    pub fn expiry(mut self, expiry: u64) -> Self {
        self.expiry = Some(expiry);
        self
    }

    pub fn min_final_cltv_expiry_delta(mut self, delta: u64) -> Self {
        self.min_final_cltv_expiry_delta = Some(delta);
        self
    }

    pub fn route_hints(mut self, hints: Vec<RouteHint>) -> Self {
        self.route_hints = Some(hints);
        self
    }
}

#[derive(Clone)]
pub struct Encoder {
    network: Currency,
    secret_key: SecretKey,
    rpc: Arc<Mutex<ClnRpc>>,
}

impl Encoder {
    pub async fn new(rpc_file: &str, network: &str) -> Result<Self> {
        Ok(Encoder {
            network: Self::parse_network(network)?,
            secret_key: SecretKey::new(&mut rand::thread_rng()),
            rpc: Arc::new(Mutex::new(ClnRpc::new(rpc_file).await?)),
        })
    }

    pub async fn encode(&self, invoice_builder: InvoiceBuilder) -> Result<String> {
        let payment_hash: sha256::Hash = Hash::from_slice(&invoice_builder.payment_hash)?;
        let payment_secret = PaymentSecret(match invoice_builder.payment_secret {
            Some(secret) => secret.as_slice().try_into()?,
            None => {
                let mut array = [0u8; 32];
                rand::rngs::OsRng.fill(&mut array[..]);
                array
            }
        });

        let mut builder = lightning_invoice::InvoiceBuilder::new(self.network.clone())
            .current_timestamp()
            .payment_hash(payment_hash)
            .payment_secret(payment_secret)
            .basic_mpp()
            .expiry_time(Duration::from_secs(
                if let Some(expiry) = invoice_builder.expiry {
                    expiry
                } else {
                    lightning_invoice::DEFAULT_EXPIRY_TIME
                },
            ))
            .min_final_cltv_expiry_delta(
                if let Some(cltv) = invoice_builder.min_final_cltv_expiry_delta {
                    cltv
                } else {
                    DEFAULT_MIN_FINAL_CLTV_EXPIRY_DELTA
                },
            );

        if let Some(amount) = invoice_builder.amount_msat {
            builder = builder.amount_milli_satoshis(amount);
        }

        if let Some(hints) = invoice_builder.route_hints {
            for hint in hints {
                builder = builder.private_route(hint);
            }
        }

        let builder = if let Some(desc) = invoice_builder.description {
            match desc {
                InvoiceDescription::Description(desc) => builder.description(desc),
                InvoiceDescription::Hash(hash) => {
                    builder.description_hash(Hash::from_slice(&hash)?)
                }
            }
        } else {
            builder.description("".into())
        };

        let invoice = builder
            .build_signed(|hash| Secp256k1::new().sign_ecdsa_recoverable(hash, &self.secret_key))?;

        let signed = self
            .rpc
            .lock()
            .await
            .call_typed(&SigninvoiceRequest {
                invstring: invoice.to_string(),
            })
            .await?;

        Ok(signed.bolt11)
    }

    fn parse_network(network: &str) -> Result<Currency> {
        match network {
            "bitcoin" => Ok(Currency::Bitcoin),
            "testnet" => Ok(Currency::BitcoinTestnet),
            "signet" => Ok(Currency::Signet),
            "regtest" => Ok(Currency::Regtest),
            _ => Err(NetworkError::InvalidNetwork.into()),
        }
    }
}
