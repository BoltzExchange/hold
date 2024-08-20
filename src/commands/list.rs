use crate::commands::structs::{parse_args, FromArr, ParamsError};
use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::database::model::{HoldInvoice, Htlc};
use crate::encoder::InvoiceEncoder;
use crate::State;
use cln_plugin::Plugin;
use lightning_invoice::Bolt11Invoice;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;

#[derive(Debug, Deserialize)]
struct ListInvoicesRequest {
    payment_hash: Option<String>,
    bolt11: Option<String>,
}

impl FromArr for ListInvoicesRequest {
    fn from_arr(arr: Vec<Value>) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        Ok(ListInvoicesRequest {
            payment_hash: if !arr.is_empty() {
                arr[0].as_str().map(|res| res.to_string())
            } else {
                None
            },
            bolt11: if arr.len() > 1 {
                arr[1].as_str().map(|res| res.to_string())
            } else {
                None
            },
        })
    }
}

#[derive(Debug, Serialize)]
struct PrettyHoldInvoice {
    pub id: i64,
    pub payment_hash: String,
    pub preimage: Option<String>,
    pub bolt11: String,
    pub state: String,
    pub created_at: chrono::NaiveDateTime,
    pub htlcs: Vec<Htlc>,
}

impl From<HoldInvoice> for PrettyHoldInvoice {
    fn from(value: HoldInvoice) -> Self {
        PrettyHoldInvoice {
            id: value.invoice.id,
            payment_hash: hex::encode(value.invoice.payment_hash),
            preimage: value.invoice.preimage.map(hex::encode),
            bolt11: value.invoice.bolt11.clone(),
            state: value.invoice.state.clone(),
            created_at: value.invoice.created_at,
            htlcs: value.htlcs.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ListInvoicesResponse {
    holdinvoices: Vec<PrettyHoldInvoice>,
}

pub async fn list_invoices<T, E>(plugin: Plugin<State<T, E>>, args: Value) -> anyhow::Result<Value>
where
    T: InvoiceHelper + Sync + Send + Clone,
    E: InvoiceEncoder + Sync + Send + Clone,
{
    let params = parse_args::<ListInvoicesRequest>(args)?;
    if params.bolt11.is_some() && params.payment_hash.is_some() {
        return Err(ParamsError::TooManyParams.into());
    }

    let payment_hash = if let Some(hash) = params.payment_hash {
        Some(hex::decode(hash)?)
    } else if let Some(invoice) = params.bolt11 {
        Some((*Bolt11Invoice::from_str(&invoice)?.payment_hash())[..].to_vec())
    } else {
        None
    };

    let invoices = match payment_hash {
        Some(hash) => match plugin.state().invoice_helper.get_by_payment_hash(&hash)? {
            Some(invoice) => vec![invoice],
            None => Vec::new(),
        },
        None => plugin.state().invoice_helper.get_all()?,
    };

    Ok(serde_json::to_value(&ListInvoicesResponse {
        holdinvoices: invoices
            .into_iter()
            .map(|e| e.into())
            .collect::<Vec<PrettyHoldInvoice>>(),
    })?)
}
