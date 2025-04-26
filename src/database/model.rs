use diesel::internal::derives::multiconnection::chrono;
use diesel::{AsChangeset, Associations, Identifiable, Insertable, Queryable, Selectable};
use serde::Serialize;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Queryable, Identifiable, Selectable, AsChangeset, Serialize, Debug, PartialEq, Clone)]
#[diesel(table_name = crate::database::schema::invoices)]
pub struct Invoice {
    pub id: i64,
    pub payment_hash: Vec<u8>,
    pub preimage: Option<Vec<u8>>,
    pub invoice: String,
    pub state: String,
    pub min_cltv: Option<i32>,
    pub created_at: chrono::NaiveDateTime,
    pub settled_at: Option<chrono::NaiveDateTime>,
}

#[derive(Insertable, Debug, PartialEq, Clone)]
#[diesel(table_name = crate::database::schema::invoices)]
pub struct InvoiceInsertable {
    pub payment_hash: Vec<u8>,
    pub invoice: String,
    pub state: String,
    pub min_cltv: Option<i32>,
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

#[derive(Debug, PartialEq)]
pub enum StateTransitionError {
    IsFinal(InvoiceState),
    InvalidTransition(InvoiceState, InvoiceState),
}

impl Display for StateTransitionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match *self {
            StateTransitionError::IsFinal(state) => write!(f, "state {state} is final"),
            StateTransitionError::InvalidTransition(old, new) => {
                write!(f, "invoice state transition ({old} -> {new})")
            }
        }
    }
}

impl Error for StateTransitionError {}

#[derive(Debug, PartialEq)]
pub enum InvoiceStateParsingError {
    InvalidInvariant(String),
}

impl Display for InvoiceStateParsingError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            InvoiceStateParsingError::InvalidInvariant(state) => {
                write!(f, "invalid invoice state invariant: {state}")
            }
        }
    }
}

impl Error for InvoiceStateParsingError {}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum InvoiceState {
    Paid = 0,
    Unpaid = 1,
    Accepted = 2,
    Cancelled = 3,
}

impl Display for InvoiceState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(Into::<String>::into(*self).as_str())
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
    type Error = InvoiceStateParsingError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "paid" => Ok(InvoiceState::Paid),
            "unpaid" => Ok(InvoiceState::Unpaid),
            "accepted" => Ok(InvoiceState::Accepted),
            "cancelled" => Ok(InvoiceState::Cancelled),
            &_ => Err(InvoiceStateParsingError::InvalidInvariant(
                value.to_string(),
            )),
        }
    }
}

impl TryFrom<&String> for InvoiceState {
    type Error = InvoiceStateParsingError;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        InvoiceState::try_from(value.as_str())
    }
}

impl InvoiceState {
    pub fn is_final(&self) -> bool {
        *self == InvoiceState::Paid || *self == InvoiceState::Cancelled
    }

    pub fn validate_transition(&self, new_state: InvoiceState) -> Result<(), StateTransitionError> {
        if self.is_final() && *self != new_state {
            return Err(StateTransitionError::IsFinal(*self));
        }

        match *self {
            InvoiceState::Unpaid => {
                if new_state != InvoiceState::Accepted && new_state != InvoiceState::Cancelled {
                    return Err(StateTransitionError::InvalidTransition(*self, new_state));
                }
            }
            InvoiceState::Accepted => {
                if new_state == InvoiceState::Unpaid {
                    return Err(StateTransitionError::InvalidTransition(*self, new_state));
                }
            }
            _ => {}
        };

        Ok(())
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

    pub fn htlc_is_known(&self, scid: &str, id: u64) -> bool {
        self.htlcs
            .iter()
            .any(|htlc| htlc.scid == scid && htlc.channel_id == id as i64)
    }
}

#[cfg(test)]
mod test {
    use crate::database::model::{
        HoldInvoice, Htlc, Invoice, InvoiceState, InvoiceStateParsingError, StateTransitionError,
    };

    #[test]
    fn invoice_state_to_string() {
        assert_eq!(InvoiceState::Paid.to_string(), "paid");
        assert_eq!(InvoiceState::Unpaid.to_string(), "unpaid");
        assert_eq!(InvoiceState::Accepted.to_string(), "accepted");
        assert_eq!(InvoiceState::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn invoice_state_from_str() {
        assert_eq!(InvoiceState::try_from("paid").unwrap(), InvoiceState::Paid);
        assert_eq!(
            InvoiceState::try_from("unpaid").unwrap(),
            InvoiceState::Unpaid
        );
        assert_eq!(
            InvoiceState::try_from("accepted").unwrap(),
            InvoiceState::Accepted
        );
        assert_eq!(
            InvoiceState::try_from("cancelled").unwrap(),
            InvoiceState::Cancelled
        );

        assert_eq!(
            InvoiceState::try_from("invalid").err().unwrap(),
            InvoiceStateParsingError::InvalidInvariant("invalid".to_string())
        );
    }

    #[test]
    fn invoice_state_from_string() {
        assert_eq!(
            InvoiceState::try_from(&String::from("paid")).unwrap(),
            InvoiceState::Paid
        );
        assert_eq!(
            InvoiceState::try_from(&String::from("unpaid")).unwrap(),
            InvoiceState::Unpaid
        );
        assert_eq!(
            InvoiceState::try_from(&String::from("accepted")).unwrap(),
            InvoiceState::Accepted
        );
        assert_eq!(
            InvoiceState::try_from(&String::from("cancelled")).unwrap(),
            InvoiceState::Cancelled
        );

        assert_eq!(
            InvoiceState::try_from(&String::from("invalid"))
                .err()
                .unwrap(),
            InvoiceStateParsingError::InvalidInvariant("invalid".to_string())
        );
    }

    #[test]
    fn invoice_state_is_final() {
        assert!(InvoiceState::Paid.is_final());
        assert!(InvoiceState::Cancelled.is_final());

        assert!(!InvoiceState::Unpaid.is_final());
        assert!(!InvoiceState::Accepted.is_final());
    }

    #[test]
    fn invoice_state_validate() {
        assert!(
            InvoiceState::Unpaid
                .validate_transition(InvoiceState::Accepted)
                .is_ok()
        );
        assert!(
            InvoiceState::Unpaid
                .validate_transition(InvoiceState::Cancelled)
                .is_ok()
        );

        assert!(
            InvoiceState::Accepted
                .validate_transition(InvoiceState::Paid)
                .is_ok()
        );
        assert!(
            InvoiceState::Unpaid
                .validate_transition(InvoiceState::Cancelled)
                .is_ok()
        );
    }

    #[test]
    fn invoice_state_validate_transition_final() {
        assert_eq!(
            InvoiceState::Paid
                .validate_transition(InvoiceState::Accepted)
                .err()
                .unwrap(),
            StateTransitionError::IsFinal(InvoiceState::Paid)
        );
        assert_eq!(
            InvoiceState::Cancelled
                .validate_transition(InvoiceState::Accepted)
                .err()
                .unwrap(),
            StateTransitionError::IsFinal(InvoiceState::Cancelled)
        );
    }

    #[test]
    fn invoice_state_validate_transition_final_same_state() {
        assert!(
            InvoiceState::Paid
                .validate_transition(InvoiceState::Paid)
                .is_ok()
        );
        assert!(
            InvoiceState::Cancelled
                .validate_transition(InvoiceState::Cancelled)
                .is_ok()
        );
    }

    #[test]
    fn invoice_state_validate_transition_invalid() {
        assert_eq!(
            InvoiceState::Unpaid
                .validate_transition(InvoiceState::Paid)
                .err()
                .unwrap(),
            StateTransitionError::InvalidTransition(InvoiceState::Unpaid, InvoiceState::Paid)
        );
        assert_eq!(
            InvoiceState::Accepted
                .validate_transition(InvoiceState::Unpaid)
                .err()
                .unwrap(),
            StateTransitionError::InvalidTransition(InvoiceState::Accepted, InvoiceState::Unpaid)
        );
    }

    #[test]
    fn hold_invoice_amount_paid_msat() {
        let mut invoice = HoldInvoice::new(
            Invoice {
                id: 0,
                payment_hash: vec![],
                preimage: None,
                invoice: "".to_string(),
                state: "".to_string(),
                min_cltv: None,
                created_at: Default::default(),
                settled_at: None,
            },
            vec![],
        );
        assert_eq!(invoice.amount_paid_msat(), 0);

        invoice.htlcs.push(Htlc {
            id: 0,
            invoice_id: 0,
            state: InvoiceState::Cancelled.to_string(),
            scid: "".to_string(),
            channel_id: 0,
            msat: 21_000,
            created_at: Default::default(),
        });
        assert_eq!(invoice.amount_paid_msat(), 0);

        invoice.htlcs.push(Htlc {
            id: 0,
            invoice_id: 0,
            state: InvoiceState::Accepted.to_string(),
            scid: "".to_string(),
            channel_id: 0,
            msat: 10_000,
            created_at: Default::default(),
        });
        assert_eq!(invoice.amount_paid_msat(), 10_000);

        invoice.htlcs.push(Htlc {
            id: 0,
            invoice_id: 0,
            state: InvoiceState::Paid.to_string(),
            scid: "".to_string(),
            channel_id: 0,
            msat: 10_000,
            created_at: Default::default(),
        });
        assert_eq!(invoice.amount_paid_msat(), 20_000);
    }

    #[test]
    fn hold_invoice_htlc_is_known() {
        let invoice = HoldInvoice::new(
            Invoice {
                id: 0,
                payment_hash: vec![],
                preimage: None,
                invoice: "".to_string(),
                state: "".to_string(),
                min_cltv: None,
                created_at: Default::default(),
                settled_at: None,
            },
            vec![
                Htlc {
                    id: 0,
                    invoice_id: 0,
                    state: InvoiceState::Accepted.to_string(),
                    scid: "asdf".to_string(),
                    channel_id: 123,
                    msat: 0,
                    created_at: Default::default(),
                },
                Htlc {
                    id: 0,
                    invoice_id: 0,
                    state: InvoiceState::Accepted.to_string(),
                    scid: "some channel".to_string(),
                    channel_id: 21,
                    msat: 0,
                    created_at: Default::default(),
                },
            ],
        );

        assert!(invoice.htlc_is_known("asdf", 123));
        assert!(invoice.htlc_is_known("some channel", 21));
        assert!(!invoice.htlc_is_known("not found", 42));
    }
}
