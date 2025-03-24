use crate::State;
use crate::commands::structs::{FromArr, ParamsError, parse_args};
use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::database::model::{HoldInvoice, Htlc};
use crate::encoder::InvoiceEncoder;
use crate::invoice::Invoice;
use cln_plugin::Plugin;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;

#[derive(Debug, Deserialize)]
struct ListInvoicesRequest {
    payment_hash: Option<String>,
    invoice: Option<String>,
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
            invoice: if arr.len() > 1 {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preimage: Option<String>,
    pub invoice: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_cltv: Option<i32>,
    pub created_at: chrono::NaiveDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settled_at: Option<chrono::NaiveDateTime>,
    pub htlcs: Vec<Htlc>,
}

impl From<HoldInvoice> for PrettyHoldInvoice {
    fn from(value: HoldInvoice) -> Self {
        PrettyHoldInvoice {
            id: value.invoice.id,
            payment_hash: hex::encode(value.invoice.payment_hash),
            preimage: value.invoice.preimage.map(hex::encode),
            invoice: value.invoice.invoice.clone(),
            state: value.invoice.state.clone(),
            min_cltv: value.invoice.min_cltv,
            created_at: value.invoice.created_at,
            settled_at: value.invoice.settled_at,
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
    if params.invoice.is_some() && params.payment_hash.is_some() {
        return Err(ParamsError::TooManyParams.into());
    }

    let payment_hash = if let Some(hash) = params.payment_hash {
        Some(hex::decode(hash)?)
    } else if let Some(invoice) = params.invoice {
        Some(Invoice::from_str(&invoice)?.payment_hash().to_vec())
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
