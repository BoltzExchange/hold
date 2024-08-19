use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::handler::Resolution;
use crate::State;
use anyhow::Result;
use cln_plugin::Plugin;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct HtlcCallbackRequest {
    pub onion: Onion,
    pub htlc: Htlc,
    pub forward_to: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Onion {
    pub payload: String,
    #[serde(rename = "type")]
    pub type_field: String,
    pub forward_msat: u64,
    pub outgoing_cltv_value: u64,
    pub total_msat: u64,
    pub next_onion: String,
    pub shared_secret: Option<String>,
    pub payment_secret: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Htlc {
    pub short_channel_id: String,
    pub id: u64,
    pub amount_msat: u64,
    pub cltv_expiry: u64,
    pub cltv_expiry_relative: u64,
    pub payment_hash: String,
}

#[derive(Debug, Serialize)]
pub enum FailureMessage {
    #[serde(rename = "0017")]
    MppTimeout,
    #[serde(rename = "400F")]
    IncorrectPaymentDetails,
}

#[derive(Debug, Serialize)]
#[serde(tag = "result")]
pub enum HtlcCallbackResponse {
    #[serde(rename = "continue")]
    Continue,
    #[serde(rename = "fail")]
    Fail { failure_message: FailureMessage },
    #[serde(rename = "resolve")]
    Resolve { payment_key: String },
}

pub async fn htlc_accepted<T>(plugin: Plugin<State<T>>, request: Value) -> Result<Value>
where
    T: InvoiceHelper + Sync + Send + Clone,
{
    let args = match serde_json::from_value::<HtlcCallbackRequest>(request) {
        Ok(args) => args,
        Err(err) => {
            error!("Could not parse htlc_accepted hook params: {}", err);
            // Continue to not crash CLN
            return Ok(serde_json::to_value(HtlcCallbackResponse::Continue)?);
        }
    };

    // Ignore forwards
    if args.forward_to.is_some() {
        debug!(
            "Ignoring forward: {}:{}",
            args.htlc.short_channel_id, args.htlc.id
        );
        return Ok(serde_json::to_value(HtlcCallbackResponse::Continue)?);
    }

    let resolution = match plugin.state().handler.clone().htlc_accepted(args).await {
        Resolution::Resolution(res) => res,
        Resolution::Resolver(solver) => solver.await.unwrap_or_else(|err| {
            error!("Could not wait for HTLC resolution: {}", err);
            HtlcCallbackResponse::Continue
        }),
    };

    Ok(serde_json::to_value(resolution)?)
}
