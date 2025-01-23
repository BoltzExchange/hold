use crate::commands::structs::{parse_args, FromArr, ParamsError};
use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::database::model::{InvoiceInsertable, InvoiceState};
use crate::encoder::InvoiceEncoder;
use crate::invoice::Invoice;
use crate::State;
use anyhow::{anyhow, Result};
use cln_plugin::Plugin;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;

#[derive(Debug, Deserialize)]
struct InjectInvoiceRequest {
    invoice: String,
}

impl FromArr for InjectInvoiceRequest {
    fn from_arr(arr: Vec<Value>) -> Result<InjectInvoiceRequest> {
        if arr.is_empty() {
            return Err(ParamsError::TooFewParams.into());
        }

        Ok(InjectInvoiceRequest {
            invoice: arr[0].as_str().ok_or(ParamsError::ParseError)?.to_string(),
        })
    }
}

#[derive(Debug, Serialize)]
struct InjectInvoiceResponse {}

pub async fn inject_invoice<T, E>(plugin: Plugin<State<T, E>>, args: Value) -> anyhow::Result<Value>
where
    T: InvoiceHelper + Sync + Send + Clone,
    E: InvoiceEncoder + Sync + Send + Clone,
{
    let params = parse_args::<InjectInvoiceRequest>(args)?;
    let invoice = Invoice::from_str(&params.invoice)?;

    // Sanity check that the invoice can go through us
    if !invoice.related_to_node(plugin.state().our_id) {
        return Err(anyhow!("invoice is not related to us"));
    }

    plugin.state().invoice_helper.insert(&InvoiceInsertable {
        invoice: params.invoice.clone(),
        payment_hash: invoice.payment_hash().to_vec(),
        state: InvoiceState::Unpaid.into(),
    })?;
    plugin.state().settler.new_invoice(
        params.invoice,
        invoice.payment_hash().to_vec(),
        invoice.amount_milli_satoshis().unwrap_or(0),
    );

    Ok(serde_json::to_value(&InjectInvoiceResponse {})?)
}
