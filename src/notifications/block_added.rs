use crate::State;
use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::encoder::InvoiceEncoder;
use cln_plugin::Plugin;
use log::{debug, error};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Debug)]
pub struct Block {
    pub height: u64,
}

#[derive(Deserialize, Debug)]
pub struct BlockAddedNotification {
    pub block_added: Block,
}

pub async fn block_added<T, E>(plugin: Plugin<State<T, E>>, request: Value) -> anyhow::Result<()>
where
    T: InvoiceHelper + Sync + Send + Clone,
    E: InvoiceEncoder + Sync + Send + Clone,
{
    let block_added_notification = match serde_json::from_value::<BlockAddedNotification>(request) {
        Ok(notification) => notification,
        Err(e) => {
            error!("Failed to parse block added notification: {e}");
            return Ok(());
        }
    };

    debug!(
        "Got block added notification: {}",
        block_added_notification.block_added.height
    );

    plugin
        .state()
        .expiry_cancel
        .clone()
        .block_added(block_added_notification.block_added.height)
        .await;

    Ok(())
}
