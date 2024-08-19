use crate::commands::structs::{parse_args, FromArr, ParamsError};
use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::State;
use cln_plugin::Plugin;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct CancelRequest {
    payment_hash: String,
}

impl FromArr for CancelRequest {
    fn from_arr(arr: Vec<Value>) -> anyhow::Result<CancelRequest> {
        if arr.is_empty() {
            return Err(ParamsError::TooFewParams.into());
        }

        Ok(CancelRequest {
            payment_hash: arr[0].as_str().ok_or(ParamsError::ParseError)?.to_string(),
        })
    }
}

#[derive(Debug, Serialize)]
struct CancelResponse {}

pub async fn cancel<T>(plugin: Plugin<State<T>>, args: Value) -> anyhow::Result<Value>
where
    T: InvoiceHelper + Sync + Send + Clone,
{
    let params = parse_args::<CancelRequest>(args)?;
    let payment_hash = hex::decode(params.payment_hash)?;

    plugin.state().settler.clone().cancel(&payment_hash).await?;

    Ok(serde_json::to_value(&CancelResponse {})?)
}
