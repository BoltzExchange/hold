use crate::commands::structs::{parse_args, FromArr, ParamsError};
use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::database::model::{InvoiceInsertable, InvoiceState};
use crate::encoder::{InvoiceBuilder, InvoiceEncoder};
use crate::State;
use anyhow::Result;
use cln_plugin::Plugin;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Debug;

#[derive(Debug, Deserialize)]
struct InvoiceRequest {
    payment_hash: String,
    amount: u64,
}

impl FromArr for InvoiceRequest {
    fn from_arr(arr: Vec<Value>) -> Result<InvoiceRequest> {
        if arr.len() < 2 {
            return Err(ParamsError::TooFewParams.into());
        }

        Ok(InvoiceRequest {
            payment_hash: arr[0].as_str().ok_or(ParamsError::ParseError)?.to_string(),
            amount: arr[1].as_u64().ok_or(ParamsError::ParseError)?,
        })
    }
}

#[derive(Debug, Serialize)]
struct InvoiceResponse {
    bolt11: String,
}

pub async fn invoice<T, E>(plugin: Plugin<State<T, E>>, args: Value) -> Result<Value>
where
    T: InvoiceHelper + Sync + Send + Clone,
    E: InvoiceEncoder + Sync + Send + Clone,
{
    let params = parse_args::<InvoiceRequest>(args)?;
    let payment_hash = hex::decode(params.payment_hash)?;

    let invoice = plugin
        .state()
        .encoder
        .encode(InvoiceBuilder::new(&payment_hash).amount_msat(params.amount))
        .await?;
    plugin.state().invoice_helper.insert(&InvoiceInsertable {
        invoice: invoice.clone(),
        payment_hash: payment_hash.clone(),
        state: InvoiceState::Unpaid.into(),
    })?;
    plugin
        .state()
        .settler
        .new_invoice(invoice.clone(), payment_hash, params.amount);

    Ok(serde_json::to_value(&InvoiceResponse { bolt11: invoice })?)
}
