use crate::database::model::{
    HoldInvoice, Htlc, HtlcInsertable, Invoice, InvoiceInsertable, InvoiceState,
};
use crate::database::schema::{htlcs, invoices};
use crate::database::Pool;
use anyhow::Result;
use diesel::{insert_into, update, BelongingToDsl, ExpressionMethods, GroupedBy};
use diesel::{QueryDsl, RunQueryDsl, SelectableHelper};

pub trait InvoiceHelper {
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
        state.validate_transition(new_state)?;

        Ok(update(invoices::dsl::invoices)
            .filter(invoices::dsl::id.eq(id))
            .set(invoices::dsl::state.eq(new_state.to_string()))
            .execute(&mut self.pool.get()?)?)
    }

    fn set_invoice_preimage(&self, id: i64, preimage: &[u8]) -> Result<usize> {
        Ok(update(invoices::dsl::invoices)
            .filter(invoices::dsl::id.eq(id))
            .set(invoices::dsl::preimage.eq(preimage))
            .execute(&mut self.pool.get()?)?)
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
            .load(&mut con)?;

        Ok(Some(HoldInvoice::new(invoice, htlcs)))
    }
}
