use crate::State;
use crate::commands::structs::{FromArr, ParamsError, parse_args};
use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::encoder::InvoiceEncoder;
use bitcoin::hashes::{Hash, sha256};
use cln_plugin::Plugin;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct SettleRequest {
    preimage: String,
}

impl FromArr for SettleRequest {
    fn from_arr(arr: Vec<Value>) -> anyhow::Result<SettleRequest> {
        if arr.is_empty() {
            return Err(ParamsError::TooFewParams.into());
        }

        Ok(SettleRequest {
            preimage: arr[0].as_str().ok_or(ParamsError::ParseError)?.to_string(),
        })
    }
}

#[derive(Debug, Serialize)]
struct SettleResponse {}

pub async fn settle<T, E>(plugin: Plugin<State<T, E>>, args: Value) -> anyhow::Result<Value>
where
    T: InvoiceHelper + Sync + Send + Clone,
    E: InvoiceEncoder + Sync + Send + Clone,
{
    let params = parse_args::<SettleRequest>(args)?;
    let preimage = hex::decode(params.preimage)?;
    let payment_hash: sha256::Hash = Hash::hash(&preimage);

    plugin
        .state()
        .settler
        .clone()
        .settle(&payment_hash[..].to_vec(), preimage.as_ref())
        .await?;

    Ok(serde_json::to_value(&SettleResponse {})?)
}
