use crate::database::model::{HoldInvoice, Htlc, InvoiceState};
use crate::grpc::service::hold;
use crate::hooks::OnionMessage;
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
            payment_hash: value.invoice.payment_hash,
            preimage: value.invoice.preimage,
            invoice: value.invoice.invoice,
            state: transform_invoice_state(
                InvoiceState::try_from(value.invoice.state.as_str()).unwrap(),
            ),
            min_cltv_expiry: value.invoice.min_cltv.map(|cltv| cltv as u64),
            created_at: value.invoice.created_at.and_utc().timestamp() as u64,
            settled_at: value
                .invoice
                .settled_at
                .map(|t| t.and_utc().timestamp() as u64),
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

impl TryFrom<OnionMessage> for hold::OnionMessage {
    type Error = anyhow::Error;

    fn try_from(value: OnionMessage) -> Result<Self, Self::Error> {
        Ok(Self {
            pathsecret: hex_from_str(value.pathsecret)?,
            reply_blindedpath: value
                .reply_blindedpath
                .map(|p| {
                    Ok::<hold::onion_message::ReplyBlindedPath, anyhow::Error>(
                        hold::onion_message::ReplyBlindedPath {
                            first_node_id: hex_from_str(p.first_node_id)?,
                            first_scid: p.first_scid,
                            first_scid_dir: p.first_scid_dir,
                            first_path_key: hex_from_str(p.first_path_key)?,
                            hops: p
                                .hops
                                .into_iter()
                                .map(|h| {
                                    Ok(hold::onion_message::reply_blinded_path::Hop {
                                        blinded_node_id: hex_from_str(h.blinded_node_id)?,
                                        encrypted_recipient_data: hex_from_str(
                                            h.encrypted_recipient_data,
                                        )?,
                                    })
                                })
                                .collect::<Result<Vec<_>, anyhow::Error>>()?,
                        },
                    )
                })
                .transpose()?,
            invoice_request: hex_from_str(value.invoice_request)?,
            invoice: hex_from_str(value.invoice)?,
            invoice_error: hex_from_str(value.invoice_error)?,
            unknown_fields: value
                .unknown_fields
                .into_iter()
                .map(|f| {
                    Ok(hold::onion_message::UnknownField {
                        number: f.number,
                        value: hex::decode(f.value).map_err(anyhow::Error::new)?,
                    })
                })
                .collect::<Result<Vec<_>, anyhow::Error>>()?,
        })
    }
}

fn hex_from_str(str: Option<String>) -> anyhow::Result<Option<Vec<u8>>> {
    str.map(|v| hex::decode(v).map_err(anyhow::Error::new))
        .transpose()
}
