use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::database::model::{HoldInvoice, HtlcInsertable, InvoiceState};
use crate::hooks::htlc_accepted::{FailureMessage, HtlcCallbackRequest, HtlcCallbackResponse};
use crate::invoice::Invoice;
use crate::settler::{Resolver, Settler};
use anyhow::Result;
use log::{debug, error, info, warn};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;

const OVERPAYMENT_FACTOR: u64 = 2;

#[derive(Debug)]
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
            error!("Could not handle HTLC: {err}");
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

        if invoice.htlc_is_known(&args.htlc.short_channel_id, args.htlc.id) {
            info!(
                "Found already accepted HTLC {}:{} for {}",
                args.htlc.short_channel_id,
                args.htlc.id,
                hex::encode(invoice.invoice.payment_hash.clone())
            );
            return Ok(Resolution::Resolver(
                self.settler
                    .add_htlc(
                        &invoice.invoice.payment_hash,
                        args.htlc.short_channel_id.clone(),
                        args.htlc.id,
                    )
                    .await,
            ));
        }

        if invoice.invoice.state != InvoiceState::Unpaid.to_string() {
            return self.reject_htlc(
                &invoice,
                &args,
                FailureMessage::IncorrectPaymentDetails,
                format!("invoice is in state: {}", invoice.invoice.state).as_str(),
            );
        }

        let invoice_decoded = Invoice::from_str(&invoice.invoice.invoice)?;

        if let Some(payment_secret) = invoice_decoded.payment_secret() {
            let htlc_secret = args.onion.payment_secret.clone().unwrap_or("".to_string());
            if htlc_secret != hex::encode(payment_secret) {
                return self.reject_htlc(
                    &invoice,
                    &args,
                    FailureMessage::IncorrectPaymentDetails,
                    "incorrect payment secret",
                );
            }
        }

        {
            let min_cltv = invoice
                .invoice
                .min_cltv
                .unwrap_or(invoice_decoded.min_final_cltv_expiry_delta() as i32);

            if args.htlc.cltv_expiry_relative < min_cltv as u64 {
                return self.reject_htlc(
                    &invoice,
                    &args,
                    FailureMessage::FinalIncorrectCltvExpiry,
                    format!(
                        "CLTV too little ({} < {})",
                        args.htlc.cltv_expiry_relative, min_cltv
                    )
                    .as_str(),
                );
            }
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
                    format!("overpayment protection ({amount_max_accepted} < {amount_paid})")
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
            self.settler
                .add_htlc(
                    &invoice.invoice.payment_hash,
                    args.htlc.short_channel_id,
                    args.htlc.id,
                )
                .await,
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

#[cfg(test)]
mod test {
    use crate::database::helpers::invoice_helper::InvoiceHelper;
    use crate::database::model::{
        HoldInvoice, HtlcInsertable, Invoice, InvoiceInsertable, InvoiceState,
    };
    use crate::handler::{Handler, Resolution};
    use crate::hooks::htlc_accepted::{
        FailureMessage, Htlc, HtlcCallbackRequest, HtlcCallbackResponse, Onion,
    };
    use crate::settler::Settler;
    use anyhow::Result;
    use lightning_invoice::Bolt11Invoice;
    use mockall::mock;
    use std::str::FromStr;

    const INVOICE: &str = "lnbc10n1pnvfs4vsp57npt9tx2glnkx29ng98cmc0lt0as8se4x4776rtwqp3gr3hj807qpp5ysnte2hh3nv4z0jd4pfe5wla956zxxg3rmxs5ux4v0xfwplvlm8sdqdw3jhxar5v4ehgxqyjw5qcqpjrzjq2rnwvp7zt9cgeparuqcrqft2kd9dm6a0z6vg0gucrqurutaezrjyrze2uqq2wcqqyqqqqdyqqqqqpqqvs9qxpqysgqjkdwjjuzfy5ek4k9xgsv0ysrc3lg349caqqh3yearxmv4zgyqhqyuntk4gyjpvpezcc66v5lyzxm240wdfgp6cqkwt7fv2nngjjnlrspmaakpk";

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

    #[tokio::test]
    async fn no_invoice() {
        let mut helper = MockInvoiceHelper::new();
        helper.expect_get_by_payment_hash().returning(|_| Ok(None));

        let mut handler = Handler::new(helper, Settler::new(MockInvoiceHelper::new(), 0));

        let res = handler
            .htlc_accepted(HtlcCallbackRequest {
                onion: Onion::default(),
                htlc: Htlc {
                    short_channel_id: "".to_string(),
                    id: 0,
                    amount_msat: 0,
                    cltv_expiry: 0,
                    cltv_expiry_relative: 0,
                    payment_hash: "00".to_string(),
                },
            })
            .await;

        match res {
            Resolution::Resolution(res) => {
                assert_eq!(res, HtlcCallbackResponse::Continue);
            }
            Resolution::Resolver(_) => {
                unreachable!();
            }
        };
    }

    #[tokio::test]
    async fn invoice_not_unpaid() {
        let mut helper = MockInvoiceHelper::new();
        helper.expect_get_by_payment_hash().returning(|_| {
            Ok(Some(HoldInvoice {
                invoice: Invoice {
                    id: 0,
                    preimage: None,
                    settled_at: None,
                    payment_hash: vec![],
                    invoice: "".to_string(),
                    created_at: Default::default(),
                    state: InvoiceState::Paid.to_string(),
                    min_cltv: None,
                },
                htlcs: vec![],
            }))
        });
        helper.expect_insert_htlc().returning(|_| Ok(0));

        let mut handler = Handler::new(helper, Settler::new(MockInvoiceHelper::new(), 0));

        let res = handler
            .htlc_accepted(HtlcCallbackRequest {
                onion: Onion::default(),
                htlc: Htlc {
                    short_channel_id: "".to_string(),
                    id: 0,
                    amount_msat: 0,
                    cltv_expiry: 0,
                    cltv_expiry_relative: 0,
                    payment_hash: "00".to_string(),
                },
            })
            .await;

        match res {
            Resolution::Resolution(res) => {
                assert_eq!(
                    res,
                    HtlcCallbackResponse::Fail {
                        failure_message: FailureMessage::IncorrectPaymentDetails
                    }
                );
            }
            Resolution::Resolver(_) => {
                unreachable!();
            }
        };
    }

    #[tokio::test]
    async fn invoice_incorrect_payment_secret() {
        let mut helper = MockInvoiceHelper::new();
        helper.expect_get_by_payment_hash().returning(|_| {
            Ok(Some(HoldInvoice {
                invoice: Invoice {
                    id: 0,
                    preimage: None,
                    settled_at: None,
                    payment_hash: vec![],
                    invoice: INVOICE.to_string(),
                    state: InvoiceState::Unpaid.to_string(),
                    created_at: Default::default(),
                    min_cltv: None,
                },
                htlcs: vec![],
            }))
        });
        helper.expect_insert_htlc().returning(|_| Ok(0));

        let mut handler = Handler::new(helper, Settler::new(MockInvoiceHelper::new(), 0));

        let res = handler
            .htlc_accepted(HtlcCallbackRequest {
                onion: Onion {
                    payload: "".to_string(),
                    total_msat: None,
                    next_onion: "".to_string(),
                    shared_secret: None,
                    payment_secret: None,
                },
                htlc: Htlc {
                    short_channel_id: "".to_string(),
                    id: 0,
                    amount_msat: 0,
                    cltv_expiry: 0,
                    cltv_expiry_relative: 0,
                    payment_hash: "00".to_string(),
                },
            })
            .await;

        match res {
            Resolution::Resolution(res) => {
                assert_eq!(
                    res,
                    HtlcCallbackResponse::Fail {
                        failure_message: FailureMessage::IncorrectPaymentDetails
                    }
                );
            }
            Resolution::Resolver(_) => {
                unreachable!();
            }
        };
    }

    #[tokio::test]
    async fn invoice_too_little_cltv_inferred() {
        let mut helper = MockInvoiceHelper::new();
        helper.expect_get_by_payment_hash().returning(|_| {
            Ok(Some(HoldInvoice {
                invoice: Invoice {
                    id: 0,
                    preimage: None,
                    settled_at: None,
                    payment_hash: vec![],
                    invoice: INVOICE.to_string(),
                    state: InvoiceState::Unpaid.to_string(),
                    min_cltv: None,
                    created_at: Default::default(),
                },
                htlcs: vec![],
            }))
        });
        helper.expect_insert_htlc().returning(|_| Ok(0));

        let mut handler = Handler::new(helper, Settler::new(MockInvoiceHelper::new(), 0));

        let res = handler
            .htlc_accepted(HtlcCallbackRequest {
                onion: Onion {
                    payload: "".to_string(),
                    total_msat: None,
                    next_onion: "".to_string(),
                    shared_secret: None,
                    payment_secret: Some(
                        "f4c2b2acca47e76328b3414f8de1ff5bfb03c335357ded0d6e006281c6f23bfc"
                            .to_string(),
                    ),
                },
                htlc: Htlc {
                    short_channel_id: "".to_string(),
                    id: 0,
                    amount_msat: 0,
                    cltv_expiry: 0,
                    cltv_expiry_relative: 2,
                    payment_hash: "00".to_string(),
                },
            })
            .await;

        match res {
            Resolution::Resolution(res) => {
                assert_eq!(
                    res,
                    HtlcCallbackResponse::Fail {
                        failure_message: FailureMessage::FinalIncorrectCltvExpiry
                    }
                );
            }
            Resolution::Resolver(_) => {
                unreachable!();
            }
        };
    }

    #[tokio::test]
    async fn invoice_too_little_cltv_explicit() {
        let min_cltv = 100;

        let mut helper = MockInvoiceHelper::new();
        helper.expect_get_by_payment_hash().returning(move |_| {
            Ok(Some(HoldInvoice {
                invoice: Invoice {
                    id: 0,
                    preimage: None,
                    settled_at: None,
                    payment_hash: vec![],
                    invoice: INVOICE.to_string(),
                    state: InvoiceState::Unpaid.to_string(),
                    min_cltv: Some(min_cltv),
                    created_at: Default::default(),
                },
                htlcs: vec![],
            }))
        });
        helper.expect_insert_htlc().returning(|_| Ok(0));

        let mut handler = Handler::new(helper, Settler::new(MockInvoiceHelper::new(), 0));

        let res = handler
            .htlc_accepted(HtlcCallbackRequest {
                onion: Onion {
                    payload: "".to_string(),
                    total_msat: None,
                    next_onion: "".to_string(),
                    shared_secret: None,
                    payment_secret: Some(
                        "f4c2b2acca47e76328b3414f8de1ff5bfb03c335357ded0d6e006281c6f23bfc"
                            .to_string(),
                    ),
                },
                htlc: Htlc {
                    short_channel_id: "".to_string(),
                    id: 0,
                    amount_msat: 0,
                    cltv_expiry: 0,
                    cltv_expiry_relative: min_cltv as u64 - 1,
                    payment_hash: "00".to_string(),
                },
            })
            .await;

        match res {
            Resolution::Resolution(res) => {
                assert_eq!(
                    res,
                    HtlcCallbackResponse::Fail {
                        failure_message: FailureMessage::FinalIncorrectCltvExpiry
                    }
                );
            }
            Resolution::Resolver(_) => {
                unreachable!();
            }
        };
    }

    #[tokio::test]
    async fn overpayment_rejection() {
        let mut helper = MockInvoiceHelper::new();
        helper.expect_get_by_payment_hash().returning(|_| {
            Ok(Some(HoldInvoice {
                invoice: Invoice {
                    id: 0,
                    preimage: None,
                    settled_at: None,
                    payment_hash: vec![],
                    invoice: INVOICE.to_string(),
                    state: InvoiceState::Unpaid.to_string(),
                    min_cltv: None,
                    created_at: Default::default(),
                },
                htlcs: vec![],
            }))
        });
        helper.expect_insert_htlc().returning(|_| Ok(0));

        let mut handler = Handler::new(helper, Settler::new(MockInvoiceHelper::new(), 0));

        let res = handler
            .htlc_accepted(HtlcCallbackRequest {
                onion: Onion {
                    payload: "".to_string(),
                    total_msat: None,
                    next_onion: "".to_string(),
                    shared_secret: None,
                    payment_secret: Some(
                        "f4c2b2acca47e76328b3414f8de1ff5bfb03c335357ded0d6e006281c6f23bfc"
                            .to_string(),
                    ),
                },
                htlc: Htlc {
                    short_channel_id: "".to_string(),
                    id: 0,
                    amount_msat: 21_000,
                    cltv_expiry: 0,
                    cltv_expiry_relative: 18,
                    payment_hash: "00".to_string(),
                },
            })
            .await;

        match res {
            Resolution::Resolution(res) => {
                assert_eq!(
                    res,
                    HtlcCallbackResponse::Fail {
                        failure_message: FailureMessage::IncorrectPaymentDetails
                    }
                );
            }
            Resolution::Resolver(_) => {
                unreachable!();
            }
        };
    }

    #[tokio::test]
    async fn accept_full_amount() {
        let invoice_decoded = Bolt11Invoice::from_str(INVOICE).unwrap();
        let payment_hash = invoice_decoded.payment_hash()[..].to_vec();
        let payment_hash_cp = payment_hash.clone();

        let mut helper = MockInvoiceHelper::new();
        helper.expect_get_by_payment_hash().returning(move |_| {
            Ok(Some(HoldInvoice {
                invoice: Invoice {
                    id: 0,
                    preimage: None,
                    settled_at: None,
                    invoice: INVOICE.to_string(),
                    created_at: Default::default(),
                    payment_hash: payment_hash_cp.clone(),
                    state: InvoiceState::Unpaid.to_string(),
                    min_cltv: None,
                },
                htlcs: vec![],
            }))
        });
        helper.expect_insert_htlc().returning(|_| Ok(0));

        let payment_hash_cp_settler = payment_hash.clone();

        let mut helper_settler = MockInvoiceHelper::new();
        helper_settler
            .expect_get_by_payment_hash()
            .returning(move |_| {
                Ok(Some(HoldInvoice {
                    invoice: Invoice {
                        id: 0,
                        preimage: None,
                        settled_at: None,
                        invoice: INVOICE.to_string(),
                        created_at: Default::default(),
                        state: InvoiceState::Unpaid.to_string(),
                        payment_hash: payment_hash_cp_settler.clone(),
                        min_cltv: None,
                    },
                    htlcs: vec![],
                }))
            });
        helper_settler
            .expect_set_htlc_states_by_invoice()
            .returning(|_, _, _| Ok(0));
        helper_settler
            .expect_set_invoice_state()
            .returning(|_, _, _| Ok(0));
        helper_settler
            .expect_set_invoice_preimage()
            .returning(|_, _| Ok(0));

        let mut handler = Handler::new(helper, Settler::new(helper_settler, 0));

        let res = handler
            .htlc_accepted(HtlcCallbackRequest {
                onion: Onion {
                    payload: "".to_string(),
                    total_msat: None,
                    next_onion: "".to_string(),
                    shared_secret: None,
                    payment_secret: Some(
                        "f4c2b2acca47e76328b3414f8de1ff5bfb03c335357ded0d6e006281c6f23bfc"
                            .to_string(),
                    ),
                },
                htlc: Htlc {
                    short_channel_id: "".to_string(),
                    id: 0,
                    amount_msat: 1_000,
                    cltv_expiry: 0,
                    cltv_expiry_relative: 18,
                    payment_hash: hex::encode(payment_hash.clone()),
                },
            })
            .await;

        match res {
            Resolution::Resolution(_) => {
                unreachable!();
            }
            Resolution::Resolver(res) => {
                let preimage = &hex::decode("0011").unwrap();
                handler
                    .settler
                    .settle(&payment_hash, preimage)
                    .await
                    .unwrap();

                assert_eq!(
                    res.await.unwrap(),
                    HtlcCallbackResponse::Resolve {
                        payment_key: hex::encode(preimage)
                    }
                );
            }
        };
    }
}
