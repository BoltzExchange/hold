use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::database::model::InvoiceState;
use crate::hooks::{FailureMessage, HtlcCallbackResponse};
use anyhow::Result;
use log::info;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

pub type Resolver = oneshot::Receiver<HtlcCallbackResponse>;
type ResolverSender = oneshot::Sender<HtlcCallbackResponse>;

#[derive(Debug)]
pub enum SettleError {
    InvoiceNotFound,
    DatabaseFetchError(anyhow::Error),
    DatabaseUpdateError(anyhow::Error),
}

impl Display for SettleError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SettleError::InvoiceNotFound => write!(f, "invoice not found"),
            SettleError::DatabaseFetchError(err) => {
                write!(f, "could not fetch invoice from database: {}", err)
            }
            SettleError::DatabaseUpdateError(err) => {
                write!(f, "could update invoice in database: {}", err)
            }
        }
    }
}

impl Error for SettleError {}

#[derive(Debug, Clone)]
pub struct Settler<T> {
    invoice_helper: T,
    pending_htlcs: Arc<Mutex<HashMap<Vec<u8>, Vec<ResolverSender>>>>,
}

impl<T> Settler<T>
where
    T: InvoiceHelper + Sync + Send + Clone,
{
    pub fn new(invoice_helper: T) -> Self {
        Settler {
            invoice_helper,
            pending_htlcs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn add_htlc(&mut self, payment_hash: &Vec<u8>) -> Resolver {
        let (tx, rx) = oneshot::channel::<HtlcCallbackResponse>();
        let mut htlcs = self.pending_htlcs.lock().await;

        if let Some(existing) = htlcs.get_mut(payment_hash) {
            existing.push(tx);
        } else {
            htlcs.insert(payment_hash.clone(), vec![tx]);
        }

        rx
    }

    pub async fn settle(
        &mut self,
        payment_hash: &Vec<u8>,
        payment_preimage: &Vec<u8>,
    ) -> Result<()> {
        let htlcs = match self.pending_htlcs.lock().await.remove(payment_hash) {
            Some(res) => res,
            None => {
                return Err(SettleError::InvoiceNotFound.into());
            }
        };
        let htlc_count = htlcs.len();

        let preimage_hex = hex::encode(payment_preimage);
        for htlc in htlcs {
            let _ = htlc.send(HtlcCallbackResponse::Resolve {
                payment_key: preimage_hex.clone(),
            });
        }

        let invoice_id = self.update_database_states(payment_hash, InvoiceState::Paid)?;
        self.invoice_helper
            .set_invoice_preimage(invoice_id, payment_preimage)?;
        info!(
            "Resolved hold invoice {} with {} HTLCs",
            hex::encode(payment_hash),
            htlc_count
        );

        Ok(())
    }

    pub async fn cancel(&mut self, payment_hash: &Vec<u8>) -> Result<()> {
        let htlcs = match self.pending_htlcs.lock().await.remove(payment_hash) {
            Some(res) => res,
            None => return Err(SettleError::InvoiceNotFound.into()),
        };
        let htlc_count = htlcs.len();

        for htlc in htlcs {
            let _ = htlc.send(HtlcCallbackResponse::Fail {
                failure_message: FailureMessage::IncorrectPaymentDetails,
            });
        }

        self.update_database_states(payment_hash, InvoiceState::Cancelled)?;
        info!(
            "Cancelled hold invoice {} with {} HTLCs",
            hex::encode(payment_hash),
            htlc_count
        );

        Ok(())
    }

    fn update_database_states(&self, payment_hash: &Vec<u8>, state: InvoiceState) -> Result<i64> {
        let invoice = match self.invoice_helper.get_by_payment_hash(payment_hash) {
            Ok(opt) => match opt {
                Some(invoice) => invoice,
                None => return Err(SettleError::InvoiceNotFound.into()),
            },
            Err(err) => return Err(SettleError::DatabaseFetchError(err).into()),
        };

        if let Err(err) = self
            .invoice_helper
            .set_invoice_state(invoice.invoice.id, state)
        {
            return Err(SettleError::DatabaseUpdateError(err).into());
        }

        if let Err(err) = self
            .invoice_helper
            .set_htlc_states_by_invoice(invoice.invoice.id, state)
        {
            return Err(SettleError::DatabaseUpdateError(err).into());
        }

        Ok(invoice.invoice.id)
    }
}
