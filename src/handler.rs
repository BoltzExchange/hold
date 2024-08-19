use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::database::model::{HoldInvoice, HtlcInsertable, InvoiceState};
use crate::hooks::{FailureMessage, HtlcCallbackRequest, HtlcCallbackResponse};
use crate::settler::{Resolver, Settler};
use anyhow::Result;
use lightning_invoice::Bolt11Invoice;
use log::{debug, error, warn};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;

// TODO: mpp timeouts

const OVERPAYMENT_FACTOR: u64 = 2;

pub enum Resolution {
    Resolution(HtlcCallbackResponse),
    Resolver(Resolver),
}

#[derive(Debug, Clone)]
pub struct Handler<T> {
    invoice_helper: T,
    lock: Arc<Mutex<()>>,
    settler: Settler<T>,
}

impl<T> Handler<T>
where
    T: InvoiceHelper + Sync + Send + Clone,
{
    pub fn new(invoice_helper: T, settler: Settler<T>) -> Self {
        Handler {
            settler,
            invoice_helper,
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub async fn htlc_accepted(&mut self, args: HtlcCallbackRequest) -> Resolution {
        self.handle_htlc(args).await.unwrap_or_else(|err| {
            error!("Could not handle HTLC: {}", err);
            // Continue to not crash CLN
            Resolution::Resolution(HtlcCallbackResponse::Continue)
        })
    }

    async fn handle_htlc(&mut self, args: HtlcCallbackRequest) -> Result<Resolution> {
        let _lock = self.lock.lock().await;

        let invoice = match self
            .invoice_helper
            .get_by_payment_hash(&hex::decode(args.htlc.payment_hash.clone())?)?
        {
            Some(invoice) => invoice,
            None => {
                debug!("No hold invoice for: {}", args.htlc.payment_hash);
                return Ok(Resolution::Resolution(HtlcCallbackResponse::Continue));
            }
        };

        // TODO: handle known htlcs

        /*
        if invoice.htlcs.is_known(htlc):
            self.handle_known_htlc(
                invoice,
                invoice.htlcs.find_htlc(htlc.short_channel_id, htlc.channel_id),
                request,
            )
            return
         */

        if invoice.invoice.state != InvoiceState::Unpaid.to_string() {
            return self.reject_htlc(
                &invoice,
                &args,
                FailureMessage::IncorrectPaymentDetails,
                format!("invoice is in state: {}", invoice.invoice.state).as_str(),
            );
        }

        let invoice_decoded = Bolt11Invoice::from_str(&invoice.invoice.bolt11)?;

        {
            let payment_secret = args.onion.payment_secret.clone().unwrap_or("".to_string());
            if payment_secret != hex::encode(invoice_decoded.payment_secret().0) {
                return self.reject_htlc(
                    &invoice,
                    &args,
                    FailureMessage::IncorrectPaymentDetails,
                    "incorrect payment secret",
                );
            }
        }

        if args.htlc.cltv_expiry_relative < invoice_decoded.min_final_cltv_expiry_delta() {
            return self.reject_htlc(
                &invoice,
                &args,
                // TODO: use incorrect_cltv_expiry or expiry_too_soon error?
                FailureMessage::IncorrectPaymentDetails,
                format!(
                    "CLTV too little ({} < {})",
                    args.htlc.cltv_expiry_relative,
                    invoice_decoded.min_final_cltv_expiry_delta()
                )
                .as_str(),
            );
        }

        let amount_paid = invoice.amount_paid_msat() + args.htlc.amount_msat;

        {
            let amount_max_accepted =
                invoice_decoded.amount_milli_satoshis().unwrap_or(0) * OVERPAYMENT_FACTOR;

            if amount_max_accepted < amount_paid {
                return self.reject_htlc(
                    &invoice,
                    &args,
                    FailureMessage::IncorrectPaymentDetails,
                    format!(
                        "overpayment protection ({} < {})",
                        amount_max_accepted, amount_paid
                    )
                    .as_str(),
                );
            }
        }

        debug!(
            "Accepted HTLC {}:{} for hold invoice {}",
            args.htlc.short_channel_id,
            args.htlc.id,
            hex::encode(invoice.invoice.payment_hash.clone())
        );
        self.invoice_helper
            .insert_htlc(&Self::create_htlc_insertable(
                InvoiceState::Accepted,
                &invoice,
                &args,
            ))?;

        if amount_paid >= invoice_decoded.amount_milli_satoshis().unwrap_or(0) {
            self.settler
                .set_accepted(&invoice.invoice, invoice.htlcs.len() + 1)?;
        }

        Ok(Resolution::Resolver(
            self.settler.add_htlc(&invoice.invoice.payment_hash).await,
        ))
    }

    fn reject_htlc(
        &self,
        invoice: &HoldInvoice,
        args: &HtlcCallbackRequest,
        failure_message: FailureMessage,
        log_message: &str,
    ) -> Result<Resolution> {
        warn!(
            "Rejected HTLC {}:{} for hold invoice {}: {}",
            args.htlc.short_channel_id,
            args.htlc.id,
            hex::encode(invoice.invoice.payment_hash.clone()),
            log_message
        );

        self.invoice_helper
            .insert_htlc(&Self::create_htlc_insertable(
                InvoiceState::Cancelled,
                invoice,
                args,
            ))?;

        Ok(Resolution::Resolution(HtlcCallbackResponse::Fail {
            failure_message,
        }))
    }

    fn create_htlc_insertable(
        state: InvoiceState,
        invoice: &HoldInvoice,
        args: &HtlcCallbackRequest,
    ) -> HtlcInsertable {
        HtlcInsertable {
            invoice_id: invoice.invoice.id,
            state: state.to_string(),
            scid: args.htlc.short_channel_id.clone(),
            channel_id: args.htlc.id as i64,
            msat: args.htlc.amount_msat as i64,
        }
    }
}
