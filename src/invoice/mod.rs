use anyhow::{Error, Result, anyhow};
use bech32::{NoChecksum, primitives::decode::CheckedHrpstring};
use bitcoin::hashes::Hash;
use lightning::{blinded_path::IntroductionNode, offers::invoice::Bolt12Invoice};
use lightning_invoice::Bolt11Invoice;
use std::str::FromStr;

const BECH32_BOLT12_INVOICE_HRP: &str = "lni";

type DecodeFunction = fn(&str) -> Result<Invoice, Error>;

const DECODE_FUNCS: &[DecodeFunction] = &[decode_bolt11, decode_bolt12_invoice];

#[derive(Debug, Clone, PartialEq)]
pub enum Invoice {
    Bolt11(Box<Bolt11Invoice>),
    Bolt12(Box<Bolt12Invoice>),
}

impl Invoice {
    pub fn payment_hash(&self) -> [u8; 32] {
        match self {
            Invoice::Bolt11(invoice) => *invoice.payment_hash().as_byte_array(),
            Invoice::Bolt12(invoice) => invoice.payment_hash().0,
        }
    }

    pub fn payment_secret(&self) -> Option<[u8; 32]> {
        match self {
            Invoice::Bolt11(invoice) => Some(invoice.payment_secret().0),
            Invoice::Bolt12(_) => None,
        }
    }

    pub fn amount_milli_satoshis(&self) -> Option<u64> {
        match self {
            Invoice::Bolt11(invoice) => invoice.amount_milli_satoshis(),
            Invoice::Bolt12(invoice) => Some(invoice.amount_msats()),
        }
    }

    pub fn min_final_cltv_expiry_delta(&self) -> u64 {
        match self {
            Invoice::Bolt11(invoice) => invoice.min_final_cltv_expiry_delta(),
            Invoice::Bolt12(invoice) => invoice
                .payment_paths()
                .iter()
                .map(|p| p.payinfo.cltv_expiry_delta)
                .min()
                .unwrap_or(0) as u64,
        }
    }

    pub fn related_to_node(&self, node_id: [u8; 33]) -> bool {
        match self {
            Invoice::Bolt11(invoice) => {
                if invoice.get_payee_pub_key().serialize() == node_id {
                    return true;
                }

                invoice.route_hints().iter().any(|hint| {
                    hint.0
                        .iter()
                        .any(|node| node.src_node_id.serialize() == node_id)
                })
            }
            Invoice::Bolt12(invoice) => {
                if invoice.signing_pubkey().serialize() == node_id {
                    return true;
                }

                invoice.payment_paths().iter().any(|path| {
                    if let IntroductionNode::NodeId(intro_node) = path.introduction_node() {
                        if intro_node.serialize() == node_id {
                            return true;
                        }
                    }

                    path.blinded_hops()
                        .iter()
                        .any(|hop| hop.blinded_node_id.serialize() == node_id)
                })
            }
        }
    }
}

impl FromStr for Invoice {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut first_error: Option<Self::Err> = None;

        for func in DECODE_FUNCS {
            match func(s) {
                Ok(res) => return Ok(res),
                Err(err) => {
                    if first_error.is_none() {
                        first_error.replace(err);
                    }
                }
            }
        }

        Err(first_error.unwrap_or(anyhow!("could not decode")))
    }
}

fn decode_bolt12_invoice(invoice: &str) -> Result<Invoice> {
    let dec = CheckedHrpstring::new::<NoChecksum>(invoice)?;
    if dec.hrp().to_lowercase() != BECH32_BOLT12_INVOICE_HRP {
        return Err(anyhow!("invalid HRP"));
    }

    let data = dec.byte_iter().collect::<Vec<_>>();
    Ok(Invoice::Bolt12(Box::new(
        Bolt12Invoice::try_from(data).map_err(|e| anyhow!("{:?}", e))?,
    )))
}

fn decode_bolt11(invoice: &str) -> Result<Invoice> {
    Ok(Invoice::Bolt11(Box::new(Bolt11Invoice::from_str(invoice)?)))
}

#[cfg(test)]
mod test {
    use bitcoin::PublicKey;

    use super::*;

    const BOLT11_INVOICE: &str = "lnbcrt1230p1pnwzkshsp584p434kjslfl030shwps75nvy4leq5k6psvdxn4kzsxjnptlmr3spp5nxqauehzqkx3xswjtrgx9lh5pqjxkyx0kszj0nc4m4jn7uk9gc5qdq8v9ekgesxqyjw5qcqp29qxpqysgqu6ft6p8c36khp082xng2xzmta25nlg803qjncal3fhzw8eshrsdyevhlgs970a09n95r3gtvqvvyk24vyv4506cu6cxl8ytaywrjkhcp468qnl";
    const BOLT12_INVOICE: &str = "lni1qqgth299fq4pg07a2jnjjxg6apy37q3qqc3xu3s3rg94nj40zfsy866mhu5vxne6tcej5878k2mneuvgjy8s5prpwdjxv93pqfjg8jssphjkw8td4vxcmrdxzrqd4sweg3uhy003cq0lljh62enfx5pqqc3xu3s3rg94nj40zfsy866mhu5vxne6tcej5878k2mneuvgjy84yqucj6q9sggzymg62fj8dfjzz3uvmft8xeufw62x0a5znkc38f0jk04wqrkvwsy6pxqzvjpu5yqdu4n36mdtpkxcmfsscrdvrk2y09ermuwqrllu47jkv6fs9tuwsyhaydearc5eyax2lmlc8apwhp8n5yynlpr4lm058y9a8f50qypsd4f70enu0x03ecscycu5d350e42x02fmtkzskzc6a453h5adh3gqx25k2jgjzxh6rdxhlmhtvm3f89wpxms2hm3cff7mkx63y7s3vp7f5xzya6lw9sc9v5hlr69pcxcvx3emt23pcqqqqqqqqqqqqqqq5qqqqqqqqqqqqqwjfvkl43fqqqqqqzjqgehz6n86sg9hp98lphy7x4rrxejy48yzs5srcfmzdqeuwzglfym8r7fmtvce7x4q8xykszczzqnys09pqr09vuwkm2cd3kx6vyxqmtqaj3rewg7lrsqlll9054nxj0cyqg5ltjmfnrm8nerkvj0uz4wfn9annnm9r3fyx4w08hj463nmya8vmutf8fmufgzvfgkyea03tltjyn2qynt8ufenhxkh5nrl5usa2f8q";

    #[test]
    fn test_from_str() {
        assert_eq!(
            Invoice::from_str(BOLT12_INVOICE).unwrap(),
            decode_bolt12_invoice(BOLT12_INVOICE).unwrap()
        );
        assert_eq!(
            Invoice::from_str(BOLT11_INVOICE).unwrap(),
            decode_bolt11(BOLT11_INVOICE).unwrap()
        );

        assert_eq!(
            Invoice::from_str("invalid").err().unwrap().to_string(),
            "Invalid bech32: parse failed"
        );
    }

    #[test]
    fn test_invoice_payment_hash() {
        assert_eq!(
            hex::encode(Invoice::from_str(BOLT11_INVOICE).unwrap().payment_hash()),
            "9981de66e2058d1341d258d062fef408246b10cfb40527cf15dd653f72c54628"
        );
        assert_eq!(
            hex::encode(Invoice::from_str(BOLT12_INVOICE).unwrap().payment_hash()),
            "b7094ff0dc9e3546336644a9c8285203c27626833c7091f493671f93b5b319f1"
        );
    }

    #[test]
    fn test_invoice_payment_secret() {
        assert_eq!(
            hex::encode(
                Invoice::from_str(BOLT11_INVOICE)
                    .unwrap()
                    .payment_secret()
                    .unwrap()
            ),
            "3d4358d6d287d3f7c5f0bb830f526c257f9052da0c18d34eb6140d29857fd8e3"
        );
        assert!(
            Invoice::from_str(BOLT12_INVOICE)
                .unwrap()
                .payment_secret()
                .is_none()
        );
    }

    #[test]
    fn test_invoice_amount_milli_satoshis() {
        assert_eq!(
            Invoice::from_str(BOLT11_INVOICE)
                .unwrap()
                .amount_milli_satoshis(),
            Some(123)
        );
        assert_eq!(
            Invoice::from_str(BOLT12_INVOICE)
                .unwrap()
                .amount_milli_satoshis(),
            Some(10000000)
        );
    }

    #[test]
    fn test_invoice_min_final_cltv_expiry_delta() {
        assert_eq!(
            Invoice::from_str(BOLT11_INVOICE)
                .unwrap()
                .min_final_cltv_expiry_delta(),
            10
        );
        assert_eq!(
            Invoice::from_str(BOLT12_INVOICE)
                .unwrap()
                .min_final_cltv_expiry_delta(),
            10,
        );
    }

    #[test]
    fn test_related_to_node_bolt11() {
        const BOLT11_ROUTING_HINT: &str = "lnbc12340n1pneza3zpp5hhjgu6far8trlutxt8pjrc62dsnpvafcwl23adt8282u2dra7wxscqzyssp5ntyh5dfyvd22lgusezf37eyq2pxatwetdnjufjr03cmxqdmty64s9q7sqqqqqqqqqqqqqqqqqqqsqqqqqysgqdqqmqz9gxqyjw5qrzjqwryaup9lh50kkranzgcdnn2fgvx390wgj5jd07rwr3vxeje0glclll3zu949263tyqqqqlgqqqqqeqqjq9tp6ckmkl3mfm8f74aeardylyreyrwwkm5r89rlea9sergw9gzt5k8tx9jjmpq8qvgjpjgz09d8fswxkxn93cyk3w8ahhs7uqd4qgkgpww9uwa";

        let invoice = Invoice::from_str(BOLT11_ROUTING_HINT).unwrap();

        assert!(
            invoice.related_to_node(
                PublicKey::from_str(
                    "0367d0a9bdc0e3d410379223e4b930806c3a2f4ee4e2c9811973f1170b52ab5159",
                )
                .unwrap()
                .inner
                .serialize()
            )
        );
        assert!(
            invoice.related_to_node(
                PublicKey::from_str(
                    "03864ef025fde8fb587d989186ce6a4a186895ee44a926bfc370e2c366597a3f8f",
                )
                .unwrap()
                .inner
                .serialize()
            )
        );

        assert!(
            !invoice.related_to_node(
                PublicKey::from_str(
                    "02c7d919b4df73e05c16973150f634e5d59bd9336b48488e1987df2f3e1737317a",
                )
                .unwrap()
                .inner
                .serialize()
            )
        );
    }

    #[test]
    fn test_related_to_node_bolt12() {
        let invoice = Invoice::from_str(BOLT12_INVOICE).unwrap();

        assert!(
            invoice.related_to_node(
                PublicKey::from_str(
                    "026483ca100de5671d6dab0d8d8da610c0dac1d94479723df1c01fffcafa566693",
                )
                .unwrap()
                .inner
                .serialize()
            )
        );
        assert!(
            invoice.related_to_node(
                PublicKey::from_str(
                    "026483ca100de5671d6dab0d8d8da610c0dac1d94479723df1c01fffcafa566693",
                )
                .unwrap()
                .inner
                .serialize()
            )
        );
        assert!(
            invoice.related_to_node(
                PublicKey::from_str(
                    "0306d53e7e67c799f1ce218263946c68fcd5467a93b5d850b0b1aed691bd3adbc5",
                )
                .unwrap()
                .inner
                .serialize()
            )
        );

        assert!(
            !invoice.related_to_node(
                PublicKey::from_str(
                    "02c7d919b4df73e05c16973150f634e5d59bd9336b48488e1987df2f3e1737317a",
                )
                .unwrap()
                .inner
                .serialize()
            )
        );
    }
}
