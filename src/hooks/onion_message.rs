use crate::State;
use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::encoder::InvoiceEncoder;
use anyhow::Result;
use cln_plugin::Plugin;
use log::error;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Clone, Debug, Hash, Deserialize)]
pub struct BlindedPathHops {
    pub blinded_node_id: Option<String>,
    pub encrypted_recipient_data: Option<String>,
}

#[derive(Clone, Debug, Hash, Deserialize)]
pub struct ReplyBlindedPath {
    pub first_node_id: Option<String>,
    pub first_scid: Option<String>,
    pub first_scid_dir: Option<u64>,
    pub first_path_key: Option<String>,
    pub hops: Vec<BlindedPathHops>,
}

#[derive(Clone, Debug, Hash, Deserialize)]
pub struct UnknownField {
    pub number: u64,
    pub value: String,
}

#[derive(Clone, Debug, Hash, Deserialize)]
pub struct OnionMessage {
    pub pathsecret: Option<String>,
    pub reply_blindedpath: Option<ReplyBlindedPath>,
    pub invoice_request: Option<String>,
    pub invoice: Option<String>,
    pub invoice_error: Option<String>,
    pub unknown_fields: Vec<UnknownField>,
}

#[derive(Debug, Hash, Deserialize)]
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

impl OnionMessage {
    pub fn id(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

pub async fn onion_message_recv<T, E>(plugin: Plugin<State<T, E>>, request: Value) -> Result<Value>
where
    T: InvoiceHelper + Sync + Send + Clone,
    E: InvoiceEncoder + Sync + Send + Clone,
{
    handle_onion_message("onion_message_recv", plugin, request).await
}

pub async fn onion_message_recv_secret<T, E>(
    plugin: Plugin<State<T, E>>,
    request: Value,
) -> Result<Value>
where
    T: InvoiceHelper + Sync + Send + Clone,
    E: InvoiceEncoder + Sync + Send + Clone,
{
    handle_onion_message("onion_message_recv_secret", plugin, request).await
}

async fn handle_onion_message<T, E>(
    name: &str,
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
            error!("Could not parse {} hook params: {}", name, err);
            return Ok(serde_json::to_value(OnionMessageResponse::Continue)?);
        }
    };

    let msg_recv = match plugin.state().messenger.received_message(msg.onion_message) {
        Some(rx) => rx,
        None => return Ok(serde_json::to_value(OnionMessageResponse::Continue)?),
    };

    Ok(serde_json::to_value(msg_recv.await.unwrap_or_else(
        |err| {
            error!("Could not wait for onion message resolution: {}", err);
            OnionMessageResponse::Continue
        },
    ))?)
}
