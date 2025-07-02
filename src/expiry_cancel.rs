use crate::Settler;
use crate::database::helpers::invoice_helper::InvoiceHelper;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct ExpiryCancel<T> {
    deadline: u64,
    settler: Settler<T>,

    lock: Arc<Mutex<()>>,
    best_height: Arc<Mutex<u64>>,
}

impl<T> ExpiryCancel<T>
where
    T: InvoiceHelper + Sync + Send + Clone,
{
    // CLN sends us block events on startup, so no need to do an initial run
    pub fn new(deadline: u64, settler: Settler<T>) -> Self {
        let cancel = Self {
            deadline,
            settler,
            lock: Arc::new(Mutex::new(())),
            best_height: Arc::new(Mutex::new(0)),
        };

        if !cancel.is_disabled() {
            log::info!(
                "Cancelling invoices when they are {} blocks from expiration",
                cancel.deadline
            );
        } else {
            log::warn!("Not cancelling invoices close to expiration");
        }

        cancel
    }

    pub async fn block_added(&mut self, block_height: u64) {
        if self.is_disabled() {
            return;
        }

        let _lock = self.lock.lock().await;

        {
            let mut best_height = self.best_height.lock().await;
            if block_height > *best_height {
                *best_height = block_height;
            } else {
                log::warn!(
                    "Added block height {} is less than best height {}",
                    block_height,
                    *best_height
                );
                return;
            }
        }

        for (payment_hash, expiry) in self.settler.get_expiries().await {
            let blocks_until_expiry = expiry - block_height;
            log::debug!(
                "Invoice {} has expiry in {} blocks",
                hex::encode(&payment_hash),
                blocks_until_expiry
            );

            if blocks_until_expiry <= self.deadline {
                log::warn!(
                    "Cancelling invoice {} because its shortest expiry is in {} blocks (deadline is {})",
                    hex::encode(&payment_hash),
                    blocks_until_expiry,
                    self.deadline
                );
                if let Err(e) = self.settler.cancel(&payment_hash).await {
                    log::error!(
                        "Could not cancel invoice {}: {e}",
                        hex::encode(&payment_hash)
                    );
                }
            }
        }
    }

    fn is_disabled(&self) -> bool {
        self.deadline == 0
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::database::helpers::invoice_helper::test::MockInvoiceHelper;
    use crate::database::model::{HoldInvoice, Invoice, InvoiceState};
    use crate::hooks::htlc_accepted::{FailureMessage, HtlcCallbackResponse};

    #[tokio::test]
    async fn test_block_added_best_height() {
        let settler = Settler::new(MockInvoiceHelper::new(), 0);
        let mut expiry_cancel = ExpiryCancel::new(2, settler);

        expiry_cancel.block_added(1).await;
        assert_eq!(*expiry_cancel.best_height.lock().await, 1);

        expiry_cancel.block_added(2).await;
        assert_eq!(*expiry_cancel.best_height.lock().await, 2);

        expiry_cancel.block_added(21).await;
        assert_eq!(*expiry_cancel.best_height.lock().await, 21);

        expiry_cancel.block_added(1).await;
        assert_eq!(*expiry_cancel.best_height.lock().await, 21);
    }

    #[tokio::test]
    async fn test_block_added_cancel() {
        let mut invoice_helper = MockInvoiceHelper::new();
        invoice_helper
            .expect_get_by_payment_hash()
            .times(1)
            .returning(|_| {
                Ok(Some(HoldInvoice {
                    invoice: Invoice {
                        id: 1,
                        payment_hash: vec![1, 2, 3],
                        state: InvoiceState::Accepted.to_string(),
                        created_at: chrono::Utc::now().naive_utc(),
                        min_cltv: Some(0),
                        invoice: "".to_string(),
                        preimage: None,
                        settled_at: None,
                    },
                    htlcs: vec![],
                }))
            });

        invoice_helper
            .expect_set_invoice_state()
            .with(
                mockall::predicate::eq(1),
                mockall::predicate::eq(InvoiceState::Accepted),
                mockall::predicate::eq(InvoiceState::Cancelled),
            )
            .times(1)
            .returning(|_, _, _| Ok(1));

        invoice_helper
            .expect_set_htlc_states_by_invoice()
            .with(
                mockall::predicate::eq(1),
                mockall::predicate::eq(InvoiceState::Accepted),
                mockall::predicate::eq(InvoiceState::Cancelled),
            )
            .times(1)
            .returning(|_, _, _| Ok(1));

        let mut settler = Settler::new(invoice_helper, 0);

        let payment_hash = vec![1, 2, 3];
        let htlc_cancel = settler.add_htlc(&payment_hash, "".to_string(), 0, 10).await;

        // To be ignored
        let htlc_ignored = settler
            .add_htlc(&vec![4, 5, 6], "".to_string(), 0, 11)
            .await;

        let mut expiry_cancel = ExpiryCancel::new(2, settler);

        expiry_cancel.block_added(8).await;
        assert_eq!(*expiry_cancel.best_height.lock().await, 8);

        assert_eq!(
            htlc_cancel.await.unwrap(),
            HtlcCallbackResponse::Fail {
                failure_message: FailureMessage::IncorrectPaymentDetails
            }
        );
        assert!(htlc_ignored.is_empty());
    }

    #[tokio::test]
    async fn test_block_added_no_cancel() {
        let mut invoice_helper = MockInvoiceHelper::new();
        invoice_helper.expect_get_by_payment_hash().times(0);

        let mut settler = Settler::new(invoice_helper, 0);

        let payment_hash = vec![1, 2, 3];
        settler.add_htlc(&payment_hash, "".to_string(), 0, 10).await;

        let mut expiry_cancel = ExpiryCancel::new(2, settler);

        expiry_cancel.block_added(1).await;
        assert_eq!(*expiry_cancel.best_height.lock().await, 1);
    }

    #[test]
    fn test_is_disabled() {
        assert!(ExpiryCancel::new(0, Settler::new(MockInvoiceHelper::new(), 0)).is_disabled());
        assert!(!ExpiryCancel::new(1, Settler::new(MockInvoiceHelper::new(), 0)).is_disabled());
        assert!(!ExpiryCancel::new(2, Settler::new(MockInvoiceHelper::new(), 0)).is_disabled());
    }
}
