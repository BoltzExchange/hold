use diesel::internal::derives::multiconnection::chrono;
use diesel::{AsChangeset, Associations, Identifiable, Insertable, Queryable, Selectable};
use serde::Serialize;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum InvoiceState {
    Paid = 0,
    Unpaid = 1,
    Accepted = 2,
    Cancelled = 3,
}

impl InvoiceState {
    pub fn to_string(self) -> String {
        Into::<String>::into(self)
    }
}

impl From<InvoiceState> for String {
    fn from(value: InvoiceState) -> Self {
        match value {
            InvoiceState::Paid => "paid",
            InvoiceState::Unpaid => "unpaid",
            InvoiceState::Accepted => "accepted",
            InvoiceState::Cancelled => "cancelled",
        }
        .to_string()
    }
}

impl TryFrom<&str> for InvoiceState {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "paid" => Ok(InvoiceState::Paid),
            "unpaid" => Ok(InvoiceState::Unpaid),
            "accepted" => Ok(InvoiceState::Accepted),
            "cancelled" => Ok(InvoiceState::Cancelled),
            &_ => Err("unknown state invariant"),
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct HoldInvoice {
    pub invoice: Invoice,
    pub htlcs: Vec<Htlc>,
}

impl HoldInvoice {
    pub fn new(invoice: Invoice, htlcs: Vec<Htlc>) -> Self {
        HoldInvoice { invoice, htlcs }
    }

    pub fn amount_paid_msat(&self) -> u64 {
        self.htlcs
            .iter()
            .filter(|htlc| {
                htlc.state == InvoiceState::Paid.to_string()
                    || htlc.state == InvoiceState::Accepted.to_string()
            })
            .map(|htlc| htlc.msat)
            .reduce(|acc, amt| acc + amt)
            .unwrap_or(0) as u64
    }
}

#[derive(Queryable, Identifiable, Selectable, AsChangeset, Serialize, Debug, PartialEq, Clone)]
#[diesel(table_name = crate::database::schema::invoices)]
pub struct Invoice {
    pub id: i64,
    pub payment_hash: Vec<u8>,
    pub preimage: Option<Vec<u8>>,
    pub bolt11: String,
    pub state: String,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Insertable, Debug, PartialEq, Clone)]
#[diesel(table_name = crate::database::schema::invoices)]
pub struct InvoiceInsertable {
    pub payment_hash: Vec<u8>,
    pub bolt11: String,
    pub state: String,
}

#[derive(
    Queryable,
    Identifiable,
    Selectable,
    Associations,
    Insertable,
    AsChangeset,
    Serialize,
    Debug,
    PartialEq,
    Clone,
)]
#[diesel(belongs_to(Invoice))]
#[diesel(table_name = crate::database::schema::htlcs)]
pub struct Htlc {
    pub id: i64,
    pub invoice_id: i64,
    pub state: String,
    pub scid: String,
    pub channel_id: i64,
    pub msat: i64,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Insertable, Debug, PartialEq, Clone)]
#[diesel(table_name = crate::database::schema::htlcs)]
pub struct HtlcInsertable {
    pub invoice_id: i64,
    pub state: String,
    pub scid: String,
    pub channel_id: i64,
    pub msat: i64,
}
