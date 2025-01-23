use crate::config::{OPTION_DATABASE, OPTION_GRPC_HOST, OPTION_GRPC_PORT, OPTION_MPP_TIMEOUT};
use crate::encoder::Encoder;
use crate::handler::Handler;
use crate::settler::Settler;
use anyhow::Result;
use cln_plugin::{Builder, RpcMethodBuilder};
use cln_rpc::model::requests::GetinfoRequest;
use cln_rpc::ClnRpc;
use log::{debug, error, info, warn};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

mod commands;
mod config;
mod database;
mod encoder;
mod grpc;
mod handler;
mod hooks;
mod invoice;
mod settler;
mod utils;

#[derive(Clone)]
struct State<T, E> {
    our_id: [u8; 33],
    handler: Handler<T>,
    settler: Settler<T>,
    encoder: E,
    invoice_helper: T,
    onion_msg_tx: broadcast::Sender<hooks::OnionMessage>,
}

#[tokio::main]
async fn main() -> Result<()> {
    std::env::set_var(
        "CLN_PLUGIN_LOG",
        "cln_plugin=trace,hold=trace,debug,info,warn,error",
    );

    info!(
        "Starting plugin {}-{}{}",
        utils::built_info::PKG_VERSION,
        utils::built_info::GIT_COMMIT_HASH_SHORT.unwrap_or(""),
        if utils::built_info::GIT_DIRTY.unwrap_or(false) {
            "-dirty"
        } else {
            ""
        }
    );

    let plugin = match Builder::new(tokio::io::stdin(), tokio::io::stdout())
        .dynamic()
        .option(OPTION_DATABASE)
        .option(OPTION_MPP_TIMEOUT)
        .option(OPTION_GRPC_HOST)
        .option(OPTION_GRPC_PORT)
        .hook("htlc_accepted", hooks::htlc_accepted)
        // TODO: do we need to stop the offers plugin of CLN itself
        .hook("onion_message_recv", hooks::onion_message_recv)
        .hook(
            "onion_message_recv_secret",
            hooks::onion_message_recv_secret,
        )
        .rpcmethod_from_builder(
            RpcMethodBuilder::new("listholdinvoices", commands::list_invoices)
                .description("Lists hold invoices")
                .usage("[payment_hash] [bolt11]"),
        )
        .rpcmethod_from_builder(
            RpcMethodBuilder::new("holdinvoice", commands::invoice)
                .description("Creates a new hold invoice")
                .usage("payment_hash amount"),
        )
        .rpcmethod_from_builder(
            RpcMethodBuilder::new("injectholdinvoice", commands::inject_invoice)
                .description("Injects a hold invoice")
                .usage("invoice"),
        )
        .rpcmethod_from_builder(
            RpcMethodBuilder::new("settleholdinvoice", commands::settle)
                .description("Settles a hold invoice")
                .usage("preimage"),
        )
        .rpcmethod_from_builder(
            RpcMethodBuilder::new("cancelholdinvoice", commands::cancel)
                .description("Cancels a hold invoice")
                .usage("payment_hash"),
        )
        .rpcmethod_from_builder(
            RpcMethodBuilder::new("cleanholdinvoices", commands::clean)
                .description("Cleans canceled hold invoices")
                .usage("[age]"),
        )
        .configure()
        .await?
    {
        Some(p) => p,
        None => return Ok(()),
    };

    let db_url = match plugin.option(&OPTION_DATABASE) {
        Ok(host) => host,
        Err(err) => {
            plugin
                .disable(format!("invalid database URL: {}", err).as_str())
                .await?;
            return Ok(());
        }
    };

    let mut mpp_timeout = match plugin.option(&OPTION_MPP_TIMEOUT) {
        Ok(timeout) => {
            if timeout < 0 {
                plugin.disable("MPP timeout has to be positive").await?;
                return Ok(());
            }

            timeout as u64
        }
        Err(err) => {
            plugin
                .disable(format!("invalid MPP timeout: {}", err).as_str())
                .await?;
            return Ok(());
        }
    };

    let grpc_host = match plugin.option(&OPTION_GRPC_HOST) {
        Ok(host) => host,
        Err(err) => {
            plugin
                .disable(format!("invalid gRPC host: {}", err).as_str())
                .await?;
            return Ok(());
        }
    };

    let grpc_port = match plugin.option(&OPTION_GRPC_PORT) {
        Ok(port) => port,
        Err(err) => {
            plugin
                .disable(format!("invalid gRPC port: {}", err).as_str())
                .await?;
            return Ok(());
        }
    };

    let config = plugin.configuration();

    let plugin_dir = Path::new(config.lightning_dir.as_str()).join("hold");
    if !plugin_dir.exists() {
        fs::create_dir(plugin_dir)?;
    }

    let db = match database::connect(&db_url) {
        Ok(db) => db,
        Err(err) => {
            plugin
                .disable(format!("could not connect to database: {}", err).as_str())
                .await?;
            return Ok(());
        }
    };

    let encoder = match Encoder::new(&config.rpc_file, &config.network).await {
        Ok(res) => res,
        Err(err) => {
            plugin
                .disable(format!("could not parse network: {}", err).as_str())
                .await?;
            return Ok(());
        }
    };

    let is_regtest = config.network == "regtest";

    if is_regtest {
        mpp_timeout = 10;
        warn!("Using MPP timeout of {} seconds on regtest", mpp_timeout);
    }

    let invoice_helper = database::helpers::invoice_helper::InvoiceHelperDatabase::new(db);
    let mut settler = Settler::new(invoice_helper.clone(), mpp_timeout);

    let our_id = ClnRpc::new(config.rpc_file)
        .await?
        .call_typed(&GetinfoRequest {})
        .await?
        .id
        .serialize();

    let (onion_msg_tx, onion_msg_rx) = broadcast::channel(1024);

    let plugin = plugin
        .start(State {
            our_id,
            onion_msg_tx,
            encoder: encoder.clone(),
            settler: settler.clone(),
            invoice_helper: invoice_helper.clone(),
            handler: Handler::new(invoice_helper.clone(), settler.clone()),
        })
        .await?;

    let cancellation_token = CancellationToken::new();

    let grpc_server = grpc::server::Server::new(
        &grpc_host,
        grpc_port,
        is_regtest,
        cancellation_token.clone(),
        std::env::current_dir()?.join(utils::built_info::PKG_NAME),
        grpc::server::State {
            our_id,
            encoder,
            invoice_helper,
            settler: settler.clone(),
            onion_msg_rx: Arc::new(onion_msg_rx),
        },
    );

    tokio::spawn(async move {
        settler.mpp_timeout_loop().await;
    });

    tokio::select! {
        _ = plugin.join() => {
            debug!("Plugin loop stopped");
        }
        res = grpc_server.start() => {
            if let Err(err) = res {
                error!("Could not start gRPC server: {}", err);
            }
        }
    }

    cancellation_token.cancel();

    info!("Stopped plugin");
    Ok(())
}
