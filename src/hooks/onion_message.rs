use crate::State;
use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::encoder::InvoiceEncoder;
use anyhow::Result;
use cln_plugin::Plugin;
use log::error;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize)]
pub struct BlindedPathHops {
    pub blinded_node_id: Option<String>,
    pub encrypted_recipient_data: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ReplyBlindedPath {
    pub first_node_id: Option<String>,
    pub first_scid: Option<String>,
    pub first_scid_dir: Option<u64>,
    pub first_path_key: Option<String>,
    pub hops: Vec<BlindedPathHops>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UnknownField {
    pub number: u64,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct OnionMessage {
    pub pathsecret: Option<String>,
    pub reply_blindedpath: Option<ReplyBlindedPath>,
    pub invoice_request: Option<String>,
    pub invoice: Option<String>,
    pub invoice_error: Option<String>,
    pub unknown_fields: Vec<UnknownField>,
}

#[derive(Debug, Deserialize)]
pub struct OnionMessageRequest {
    pub onion_message: OnionMessage,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "result")]
pub enum OnionMessageResponse {
    #[serde(rename = "continue")]
    Continue,
    #[serde(rename = "resolve")]
    Resolve,
}

pub async fn onion_message_recv<T, E>(plugin: Plugin<State<T, E>>, request: Value) -> Result<Value>
where
    T: InvoiceHelper + Sync + Send + Clone,
    E: InvoiceEncoder + Sync + Send + Clone,
{
    let msg = match serde_json::from_value::<OnionMessageRequest>(request) {
        Ok(args) => args,
        Err(err) => {
            error!("Could not parse onion_message_recv hook params: {}", err);
            return Ok(serde_json::to_value(OnionMessageResponse::Continue)?);
        }
    };
    plugin.state().onion_msg_tx.send(msg.onion_message)?;

    Ok(serde_json::to_value(OnionMessageResponse::Resolve)?)
}

pub async fn onion_message_recv_secret<T, E>(
    plugin: Plugin<State<T, E>>,
    request: Value,
) -> Result<Value>
where
    T: InvoiceHelper + Sync + Send + Clone,
    E: InvoiceEncoder + Sync + Send + Clone,
{
    let msg = match serde_json::from_value::<OnionMessageRequest>(request) {
        Ok(args) => args,
        Err(err) => {
            error!(
                "Could not parse onion_message_recv_secret hook params: {}",
                err
            );
            return Ok(serde_json::to_value(OnionMessageResponse::Continue)?);
        }
    };
    plugin.state().onion_msg_tx.send(msg.onion_message)?;

    Ok(serde_json::to_value(OnionMessageResponse::Resolve)?)
}
