use crate::database::model::{
    HoldInvoice, Htlc, HtlcInsertable, Invoice, InvoiceInsertable, InvoiceState,
};
use crate::database::schema::{htlcs, invoices};
use crate::database::{AnyConnection, Pool};
use anyhow::{Result, anyhow};
use chrono::{TimeDelta, Utc};
use diesel::dsl::delete;
use diesel::r2d2::{ConnectionManager, PooledConnection};
use diesel::{
    BelongingToDsl, BoolExpressionMethods, Connection, ExpressionMethods, GroupedBy, insert_into,
    update,
};
use diesel::{QueryDsl, RunQueryDsl, SelectableHelper};
use std::ops::Sub;

pub trait InvoiceHelper {
    fn insert(&self, invoice: &InvoiceInsertable) -> Result<usize>;
    fn insert_htlc(&self, htlc: &HtlcInsertable) -> Result<usize>;

    fn set_invoice_state(
        &self,
        id: i64,
        state: InvoiceState,
        new_state: InvoiceState,
    ) -> Result<usize>;
    fn set_invoice_settled(&self, payment_hash: &[u8], preimage: &[u8]) -> Result<()>;

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

#[derive(Clone, Debug)]
pub struct InvoiceHelperDatabase {
    pool: Pool,
}

impl InvoiceHelperDatabase {
    pub fn new(pool: Pool) -> Self {
        InvoiceHelperDatabase { pool }
    }

    fn set_invoice_state(
        con: &mut PooledConnection<ConnectionManager<AnyConnection>>,
        id: i64,
        state: InvoiceState,
        new_state: InvoiceState,
    ) -> Result<usize> {
        state.validate_transition(new_state)?;

        if new_state != InvoiceState::Paid {
            Ok(update(invoices::dsl::invoices)
                .filter(invoices::dsl::id.eq(id))
                .set(invoices::dsl::state.eq(new_state.to_string()))
                .execute(con)?)
        } else {
            Ok(update(invoices::dsl::invoices)
                .filter(invoices::dsl::id.eq(id))
                .set((
                    invoices::dsl::state.eq(new_state.to_string()),
                    invoices::dsl::settled_at.eq(Some(Utc::now().naive_utc())),
                ))
                .execute(con)?)
        }
    }
}

impl InvoiceHelper for InvoiceHelperDatabase {
    fn insert(&self, invoice: &InvoiceInsertable) -> Result<usize> {
        Ok(insert_into(invoices::dsl::invoices)
            .values(invoice)
            .execute(&mut self.pool.get()?)?)
    }

    fn insert_htlc(&self, htlc: &HtlcInsertable) -> Result<usize> {
        Ok(insert_into(htlcs::dsl::htlcs)
            .values(htlc)
            .execute(&mut self.pool.get()?)?)
    }

    fn set_invoice_state(
        &self,
        id: i64,
        state: InvoiceState,
        new_state: InvoiceState,
    ) -> Result<usize> {
        Self::set_invoice_state(&mut self.pool.get()?, id, state, new_state)
    }

    fn set_invoice_settled(&self, payment_hash: &[u8], preimage: &[u8]) -> Result<()> {
        let mut con = self.pool.get()?;
        con.transaction(|tx| {
            let invoice = invoices::dsl::invoices
                .filter(invoices::dsl::payment_hash.eq(payment_hash))
                .first::<Invoice>(tx)?;

            Self::set_invoice_state(
                tx,
                invoice.id,
                InvoiceState::try_from(&invoice.state)?,
                InvoiceState::Paid,
            )?;
            update(invoices::dsl::invoices)
                .filter(invoices::dsl::id.eq(invoice.id))
                .set(invoices::dsl::preimage.eq(preimage))
                .execute(tx)?;

            update(htlcs::dsl::htlcs)
                .filter(
                    htlcs::dsl::invoice_id
                        .eq(invoice.id)
                        // Only settle accepted HTLCs
                        .and(htlcs::dsl::state.eq(InvoiceState::Accepted.to_string())),
                )
                .set(htlcs::dsl::state.eq(InvoiceState::Paid.to_string()))
                .execute(tx)?;

            Ok(())
        })
    }

    fn set_htlc_state_by_id(
        &self,
        htlc_id: i64,
        state: InvoiceState,
        new_state: InvoiceState,
    ) -> Result<usize> {
        state.validate_transition(new_state)?;

        Ok(update(htlcs::dsl::htlcs)
            .filter(htlcs::dsl::id.eq(htlc_id))
            .set(htlcs::dsl::state.eq(new_state.to_string()))
            .execute(&mut self.pool.get()?)?)
    }

    fn set_htlc_states_by_invoice(
        &self,
        invoice_id: i64,
        state: InvoiceState,
        new_state: InvoiceState,
    ) -> Result<usize> {
        state.validate_transition(new_state)?;

        Ok(update(htlcs::dsl::htlcs)
            .filter(htlcs::dsl::invoice_id.eq(invoice_id))
            .set(htlcs::dsl::state.eq(new_state.to_string()))
            .execute(&mut self.pool.get()?)?)
    }

    fn clean_cancelled(&self, age: Option<u64>) -> Result<usize> {
        let age = match TimeDelta::new(age.unwrap_or(0) as i64, 0) {
            Some(age) => age,
            None => return Err(anyhow!("invalid age")),
        };

        let now = Utc::now().naive_utc().sub(age);

        let mut con = self.pool.get()?;
        con.transaction(|tx| {
            let invoice_clause = invoices::dsl::state
                .eq(InvoiceState::Cancelled.to_string())
                .and(invoices::dsl::created_at.le(now));

            let invoices = invoices::dsl::invoices
                .select(Invoice::as_select())
                .filter(invoice_clause.clone())
                .load(tx)?;

            delete(
                htlcs::dsl::htlcs
                    .filter(htlcs::dsl::invoice_id.eq_any(invoices.iter().map(|i| i.id))),
            )
            .execute(tx)?;

            Ok(delete(invoices::dsl::invoices.filter(invoice_clause)).execute(tx)?)
        })
    }

    fn get_all(&self) -> Result<Vec<HoldInvoice>> {
        let mut con = self.pool.get()?;

        let invoices = invoices::dsl::invoices
            .select(Invoice::as_select())
            .order_by(invoices::dsl::id)
            .load(&mut con)?;
        let htlcs = Htlc::belonging_to(&invoices)
            .select(Htlc::as_select())
            .load(&mut con)?;

        Ok(htlcs
            .grouped_by(&invoices)
            .into_iter()
            .zip(invoices)
            .map(|(htlcs, invoice)| HoldInvoice::new(invoice, htlcs))
            .collect())
    }

    fn get_paginated(&self, index_start: i64, limit: u64) -> Result<Vec<HoldInvoice>> {
        let mut con = self.pool.get()?;

        let invoices = invoices::dsl::invoices
            .select(Invoice::as_select())
            .filter(invoices::dsl::id.ge(index_start))
            .order_by(invoices::dsl::id)
            .limit(limit as i64)
            .load(&mut con)?;
        let htlcs = Htlc::belonging_to(&invoices)
            .select(Htlc::as_select())
            .load(&mut con)?;

        Ok(htlcs
            .grouped_by(&invoices)
            .into_iter()
            .zip(invoices)
            .map(|(htlcs, invoice)| HoldInvoice::new(invoice, htlcs))
            .collect())
    }

    fn get_by_payment_hash(&self, payment_hash: &[u8]) -> Result<Option<HoldInvoice>> {
        let mut con = self.pool.get()?;

        let invoices = invoices::dsl::invoices
            .select(Invoice::as_select())
            .filter(invoices::dsl::payment_hash.eq(payment_hash))
            .limit(1)
            .load(&mut con)?;

        if invoices.is_empty() {
            return Ok(None);
        }

        let invoice = invoices[0].clone();
        let htlcs = Htlc::belonging_to(&vec![invoice.clone()])
            .select(Htlc::as_select())
            .order_by(htlcs::dsl::id)
            .load(&mut con)?;

        Ok(Some(HoldInvoice::new(invoice, htlcs)))
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::database::connect;
    use mockall::mock;

    mock! {
        pub InvoiceHelper {}

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
            fn set_invoice_settled(&self, payment_hash: &[u8], preimage: &[u8]) -> Result<()>;
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

    #[test]
    fn test_set_invoice_settled() {
        let pool = connect("sqlite://:memory:").unwrap();
        let helper = InvoiceHelperDatabase::new(pool);

        let payment_hash = vec![1, 2, 3];
        let invoice = InvoiceInsertable {
            payment_hash: payment_hash.clone(),
            state: InvoiceState::Accepted.to_string(),
            min_cltv: None,
            invoice: "ln".to_string(),
        };

        helper.insert(&invoice).unwrap();

        let htlc_accepted = HtlcInsertable {
            invoice_id: 1,
            state: InvoiceState::Accepted.to_string(),
            scid: "1".to_string(),
            channel_id: 1,
            msat: 1000,
        };
        helper.insert_htlc(&htlc_accepted).unwrap();

        let htlc_cancelled = HtlcInsertable {
            invoice_id: 1,
            state: InvoiceState::Cancelled.to_string(),
            scid: "2".to_string(),
            channel_id: 2,
            msat: 1000,
        };
        helper.insert_htlc(&htlc_cancelled).unwrap();

        let preimage = &[1, 2, 3];
        helper.set_invoice_settled(&payment_hash, preimage).unwrap();

        let invoice = helper.get_by_payment_hash(&payment_hash).unwrap().unwrap();
        assert_eq!(invoice.invoice.state, InvoiceState::Paid.to_string());
        assert_eq!(invoice.invoice.preimage, Some(preimage.to_vec()));

        assert_eq!(invoice.htlcs.len(), 2);
        assert_eq!(invoice.htlcs[0].state, InvoiceState::Paid.to_string());
        assert_eq!(invoice.htlcs[1].state, InvoiceState::Cancelled.to_string());
    }
}
