use crate::database::model::{HoldInvoice, Htlc, InvoiceState};
use crate::grpc::service::hold;
use lightning_invoice::{RouteHint, RouteHintHop, RoutingFees};
use secp256k1::{Error, PublicKey};

impl From<Htlc> for hold::Htlc {
    fn from(value: Htlc) -> Self {
        hold::Htlc {
            id: value.id,
            state: transform_invoice_state(InvoiceState::try_from(value.state.as_str()).unwrap()),
            scid: value.scid,
            channel_id: value.channel_id as u64,
            msat: value.msat as u64,
            created_at: value.created_at.and_utc().timestamp() as u64,
        }
    }
}

impl From<HoldInvoice> for hold::Invoice {
    fn from(value: HoldInvoice) -> Self {
        hold::Invoice {
            id: value.invoice.id,
            payment_hash: vec![],
            preimage: value.invoice.preimage,
            bolt11: value.invoice.bolt11,
            state: transform_invoice_state(
                InvoiceState::try_from(value.invoice.state.as_str()).unwrap(),
            ),
            created_at: value.invoice.created_at.and_utc().timestamp() as u64,
            htlcs: value.htlcs.into_iter().map(|htlc| htlc.into()).collect(),
        }
    }
}

pub fn transform_invoice_state(value: InvoiceState) -> i32 {
    match value {
        InvoiceState::Paid => hold::InvoiceState::Paid,
        InvoiceState::Unpaid => hold::InvoiceState::Unpaid,
        InvoiceState::Accepted => hold::InvoiceState::Accepted,
        InvoiceState::Cancelled => hold::InvoiceState::Cancelled,
    }
    .into()
}

pub fn transform_route_hints(hints: Vec<hold::RoutingHint>) -> Result<Vec<RouteHint>, Error> {
    let mut res = Vec::new();

    for hint in hints.into_iter() {
        match transform_route_hint(hint) {
            Ok(hint) => res.push(hint),
            Err(err) => return Err(err),
        };
    }

    Ok(res)
}

fn transform_route_hint(hint: hold::RoutingHint) -> Result<RouteHint, Error> {
    let hints = hint
        .hops
        .into_iter()
        .map(|hop| {
            Ok(RouteHintHop {
                src_node_id: match PublicKey::from_slice(&hop.public_key) {
                    Ok(key) => key,
                    Err(err) => return Err(err),
                },
                short_channel_id: hop.short_channel_id,
                fees: RoutingFees {
                    base_msat: hop.base_fee as u32,
                    proportional_millionths: hop.ppm_fee as u32,
                },
                cltv_expiry_delta: hop.cltv_expiry_delta as u16,
                htlc_minimum_msat: None,
                htlc_maximum_msat: None,
            })
        })
        .collect::<Vec<Result<RouteHintHop, Error>>>();

    for hint in &hints {
        if let Err(err) = hint {
            return Err(*err);
        }
    }

    Ok(RouteHint(
        hints.into_iter().map(|hint| hint.unwrap()).collect(),
    ))
}
