use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::database::model::{HoldInvoice, Invoice, InvoiceState};
use crate::hooks::htlc_accepted::{FailureMessage, HtlcCallbackResponse};
use anyhow::Result;
use log::{info, trace, warn};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::Sub;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::time;

const MPP_INTERVAL_SECONDS: u64 = 15;

pub type Resolver = oneshot::Receiver<HtlcCallbackResponse>;
type ResolverSender = oneshot::Sender<HtlcCallbackResponse>;

#[derive(Debug)]
pub enum SettleError {
    NoHtlcsToSettle,
    InvoiceNotFound,
    DatabaseFetchError(anyhow::Error),
    DatabaseUpdateError(anyhow::Error),
}

impl Display for SettleError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SettleError::NoHtlcsToSettle => write!(f, "no HTLCs to settle"),
            SettleError::InvoiceNotFound => write!(f, "invoice not found"),
            SettleError::DatabaseFetchError(err) => {
                write!(f, "could not fetch invoice from database: {}", err)
            }
            SettleError::DatabaseUpdateError(err) => {
                write!(f, "could not update invoice in database: {}", err)
            }
        }
    }
}

impl Error for SettleError {}

#[derive(Debug)]
pub struct PendingHtlc {
    scid: String,
    channel_id: u64,
    sender: ResolverSender,
    time: SystemTime,
}

#[derive(Debug, Clone)]
pub struct StateUpdate {
    pub payment_hash: Vec<u8>,
    pub bolt11: String,
    pub state: InvoiceState,
}

#[derive(Debug, Clone)]
pub struct Settler<T> {
    invoice_helper: T,
    mpp_timeout: Duration,
    state_tx: broadcast::Sender<StateUpdate>,
    pending_htlcs: Arc<Mutex<HashMap<Vec<u8>, Vec<PendingHtlc>>>>,
}

impl<T> Settler<T>
where
    T: InvoiceHelper + Sync + Send + Clone,
{
    pub fn new(invoice_helper: T, mpp_timeout: u64) -> Self {
        let (state_tx, _) = broadcast::channel(128);
        Settler {
            state_tx,
            invoice_helper,
            mpp_timeout: Duration::from_secs(mpp_timeout),
            pending_htlcs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn state_rx(&self) -> broadcast::Receiver<StateUpdate> {
        self.state_tx.subscribe()
    }

    pub fn new_invoice(&self, invoice: String, payment_hash: Vec<u8>, amount_msat: u64) {
        info!(
            "Added hold invoice {} for {}msat",
            hex::encode(payment_hash.clone()),
            amount_msat
        );

        let _ = self.state_tx.send(StateUpdate {
            payment_hash,
            bolt11: invoice,
            state: InvoiceState::Unpaid,
        });
    }

    pub fn set_accepted(&self, invoice: &Invoice, num_htlcs: usize) -> Result<()> {
        info!(
            "Accepted hold invoice {} with {} HTLCs",
            hex::encode(invoice.payment_hash.clone()),
            num_htlcs
        );
        self.invoice_helper.set_invoice_state(
            invoice.id,
            InvoiceState::try_from(&invoice.state)?,
            InvoiceState::Accepted,
        )?;
        let _ = self.state_tx.send(StateUpdate {
            state: InvoiceState::Accepted,
            bolt11: invoice.invoice.clone(),
            payment_hash: invoice.payment_hash.clone(),
        });

        Ok(())
    }

    pub async fn add_htlc(
        &mut self,
        payment_hash: &Vec<u8>,
        scid: String,
        channel_id: u64,
    ) -> Resolver {
        let (tx, rx) = oneshot::channel::<HtlcCallbackResponse>();
        let mut htlcs = self.pending_htlcs.lock().await;

        let pending = PendingHtlc {
            scid,
            channel_id,
            sender: tx,
            time: SystemTime::now(),
        };

        if let Some(existing) = htlcs.get_mut(payment_hash) {
            existing.push(pending);
        } else {
            htlcs.insert(payment_hash.clone(), vec![pending]);
        }

        rx
    }

    pub async fn settle(
        &mut self,
        payment_hash: &Vec<u8>,
        payment_preimage: &Vec<u8>,
    ) -> Result<()> {
        if self.get_invoice(payment_hash)?.invoice.state == InvoiceState::Paid.to_string() {
            return Ok(());
        }

        let htlcs = match self.pending_htlcs.lock().await.remove(payment_hash) {
            Some(res) => res,
            None => {
                return Err(SettleError::NoHtlcsToSettle.into());
            }
        };
        let htlc_count = htlcs.len();

        let preimage_hex = hex::encode(payment_preimage);
        for htlc in htlcs {
            let _ = htlc.sender.send(HtlcCallbackResponse::Resolve {
                payment_key: preimage_hex.clone(),
            });
        }

        let (invoice_id, bolt11) = self.update_database_states(payment_hash, InvoiceState::Paid)?;
        self.invoice_helper
            .set_invoice_preimage(invoice_id, payment_preimage)?;
        let _ = self.state_tx.send(StateUpdate {
            bolt11,
            state: InvoiceState::Paid,
            payment_hash: payment_hash.clone(),
        });
        info!(
            "Resolved hold invoice {} with {} HTLCs",
            hex::encode(payment_hash),
            htlc_count
        );

        Ok(())
    }

    pub async fn cancel(&mut self, payment_hash: &Vec<u8>) -> Result<()> {
        let htlcs = self
            .pending_htlcs
            .lock()
            .await
            .remove(payment_hash)
            .unwrap_or_else(Vec::new);
        let htlc_count = htlcs.len();

        for htlc in htlcs {
            let _ = htlc.sender.send(HtlcCallbackResponse::Fail {
                failure_message: FailureMessage::IncorrectPaymentDetails,
            });
        }

        let (_, bolt11) = self.update_database_states(payment_hash, InvoiceState::Cancelled)?;
        let _ = self.state_tx.send(StateUpdate {
            bolt11,
            state: InvoiceState::Cancelled,
            payment_hash: payment_hash.clone(),
        });
        info!(
            "Cancelled hold invoice {} with {} pending HTLCs",
            hex::encode(payment_hash),
            htlc_count
        );

        Ok(())
    }

    pub async fn mpp_timeout_loop(&mut self) {
        info!(
            "Checking for MPP timeouts every {} seconds",
            MPP_INTERVAL_SECONDS
        );
        let mut interval = time::interval(Duration::from_secs(MPP_INTERVAL_SECONDS));

        loop {
            interval.tick().await;
            trace!("Checking for MPP timeouts");

            let now = SystemTime::now();

            for (payment_hash, pending) in self.pending_htlcs.lock().await.iter_mut() {
                let invoice = match self.invoice_helper.get_by_payment_hash(payment_hash) {
                    Ok(invoice) => match invoice {
                        Some(invoice) => invoice,
                        None => {
                            warn!(
                                "Not database entry found for invoice: {}",
                                hex::encode(payment_hash)
                            );
                            continue;
                        }
                    },
                    Err(err) => {
                        warn!("Could not fetch invoice: {}", err);
                        continue;
                    }
                };

                if invoice.invoice.state == InvoiceState::Accepted.to_string() {
                    continue;
                }

                for i in (0..pending.len()).rev() {
                    let htlc = &pending[i];
                    let since_accepted = match now.duration_since(htlc.time) {
                        Ok(since) => since,
                        Err(err) => {
                            warn!("Could not compare time since HTLC was accepted: {}", err);
                            continue;
                        }
                    };

                    if since_accepted < self.mpp_timeout {
                        trace!(
                            "Cancelling payment part {}:{} of {} with MPP timeout in {:?}",
                            htlc.scid,
                            htlc.channel_id,
                            hex::encode(payment_hash),
                            self.mpp_timeout.sub(since_accepted)
                        );
                        continue;
                    }

                    let htlc = pending.remove(i);
                    let _ = htlc.sender.send(HtlcCallbackResponse::Fail {
                        failure_message: FailureMessage::MppTimeout,
                    });
                    let htlc_db = match invoice
                        .htlcs
                        .iter()
                        .find(|h| h.scid == htlc.scid && h.channel_id as u64 == htlc.channel_id)
                    {
                        Some(htlc) => htlc,
                        None => {
                            warn!(
                                "Could not find HTLC {}:{} of {} in database",
                                htlc.scid,
                                htlc.channel_id,
                                hex::encode(payment_hash)
                            );
                            continue;
                        }
                    };

                    if let Err(err) = self.invoice_helper.set_htlc_state_by_id(
                        htlc_db.id,
                        match InvoiceState::try_from(&htlc_db.state) {
                            Ok(state) => state,
                            Err(err) => {
                                warn!("Could not parse HTLC database state: {}", err);
                                continue;
                            }
                        },
                        InvoiceState::Cancelled,
                    ) {
                        warn!(
                            "Could not update database state of HTLC of {}: {}",
                            hex::encode(payment_hash),
                            err
                        );
                        continue;
                    };

                    info!(
                        "Cancelled payment part {}:{} of {} with MPP timeout",
                        htlc.scid,
                        htlc.channel_id,
                        hex::encode(payment_hash)
                    );
                }
            }
        }
    }

    fn update_database_states(
        &self,
        payment_hash: &[u8],
        state: InvoiceState,
    ) -> Result<(i64, String)> {
        let invoice = self.get_invoice(payment_hash)?;
        let current_state = InvoiceState::try_from(&invoice.invoice.state)?;

        if let Err(err) =
            self.invoice_helper
                .set_invoice_state(invoice.invoice.id, current_state, state)
        {
            return Err(SettleError::DatabaseUpdateError(err).into());
        }

        if let Err(err) =
            self.invoice_helper
                .set_htlc_states_by_invoice(invoice.invoice.id, current_state, state)
        {
            return Err(SettleError::DatabaseUpdateError(err).into());
        }

        Ok((invoice.invoice.id, invoice.invoice.invoice))
    }

    fn get_invoice(&self, payment_hash: &[u8]) -> Result<HoldInvoice> {
        match self.invoice_helper.get_by_payment_hash(payment_hash) {
            Ok(opt) => match opt {
                Some(invoice) => Ok(invoice),
                None => Err(SettleError::InvoiceNotFound.into()),
            },
            Err(err) => Err(SettleError::DatabaseFetchError(err).into()),
        }
    }
}
