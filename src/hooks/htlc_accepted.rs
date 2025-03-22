use crate::State;
use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::encoder::InvoiceEncoder;
use crate::handler::Resolution;
use anyhow::Result;
use cln_plugin::Plugin;
use log::error;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct HtlcCallbackRequest {
    pub onion: Onion,
    pub htlc: Htlc,
}

#[allow(dead_code)]
#[derive(Default, Debug, Deserialize)]
pub struct Onion {
    pub payload: String,
    pub total_msat: Option<u64>,
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

#[derive(Debug, PartialEq, Serialize)]
pub enum FailureMessage {
    #[serde(rename = "0017")]
    MppTimeout,
    #[serde(rename = "400F")]
    IncorrectPaymentDetails,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "result")]
pub enum HtlcCallbackResponse {
    #[serde(rename = "continue")]
    Continue,
    #[serde(rename = "fail")]
    Fail { failure_message: FailureMessage },
    #[serde(rename = "resolve")]
    Resolve { payment_key: String },
}

pub async fn htlc_accepted<T, E>(plugin: Plugin<State<T, E>>, request: Value) -> Result<Value>
where
    T: InvoiceHelper + Sync + Send + Clone,
    E: InvoiceEncoder + Sync + Send + Clone,
{
    let args = match serde_json::from_value::<HtlcCallbackRequest>(request) {
        Ok(args) => args,
        Err(err) => {
            error!("Could not parse htlc_accepted hook params: {}", err);
            // Continue to not crash CLN
            return Ok(serde_json::to_value(HtlcCallbackResponse::Continue)?);
        }
    };

    // Forwards are not ignored anymore because there could be a next hop for BOLT12 invoices
    let resolution = match plugin.state().handler.clone().htlc_accepted(args).await {
        Resolution::Resolution(res) => res,
        Resolution::Resolver(solver) => solver.await.unwrap_or_else(|err| {
            error!("Could not wait for HTLC resolution: {}", err);
            HtlcCallbackResponse::Continue
        }),
    };

    Ok(serde_json::to_value(resolution)?)
}
