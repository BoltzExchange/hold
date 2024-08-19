use crate::config::{OPTION_DATABASE, OPTION_GRPC_HOST, OPTION_GRPC_PORT};
use crate::encoder::Encoder;
use crate::handler::Handler;
use crate::settler::Settler;
use anyhow::Result;
use cln_plugin::Builder;
use log::{debug, error};

mod commands;
mod config;
mod database;
mod encoder;
mod grpc;
mod handler;
mod hooks;
mod settler;
mod utils;

#[derive(Clone)]
struct State<T> {
    handler: Handler<T>,
    settler: Settler<T>,
    encoder: Encoder,
    invoice_helper: T,
}

// TODO: backfill records from old datastore

#[tokio::main]
async fn main() -> Result<()> {
    // TODO: what should be used here instead?
    std::env::set_var(
        "CLN_PLUGIN_LOG",
        "cln_plugin=debug,hold=trace,debug,info,warn,error",
    );

    debug!("Starting plugin");

    // TODO: graceful shutdown
    let plugin = match Builder::new(tokio::io::stdin(), tokio::io::stdout())
        .dynamic()
        .option(OPTION_DATABASE)
        .option(OPTION_GRPC_HOST)
        .option(OPTION_GRPC_PORT)
        .hook("htlc_accepted", hooks::htlc_accepted)
        .rpcmethod(
            "listholdinvoices",
            "Lists hold invoices",
            commands::list_invoices,
        )
        .rpcmethod(
            "holdinvoice",
            "Creates a new hold invoice",
            commands::invoice,
        )
        .rpcmethod(
            "settleholdinvoice",
            "Settles a hold invoice",
            commands::settle,
        )
        .rpcmethod(
            "cancelholdinvoice",
            "Cancels a hold invoice",
            commands::cancel,
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

    let db = match database::connect(&db_url) {
        Ok(db) => db,
        Err(err) => {
            plugin
                .disable(format!("could not connect to database: {}", err).as_str())
                .await?;
            return Ok(());
        }
    };

    let config = plugin.configuration();
    let encoder = match Encoder::new(&config.rpc_file, &config.network).await {
        Ok(res) => res,
        Err(err) => {
            plugin
                .disable(format!("could not parse network: {}", err).as_str())
                .await?;
            return Ok(());
        }
    };

    let invoice_helper = database::helpers::invoice_helper::InvoiceHelperDatabase::new(db);
    let settler = Settler::new(invoice_helper.clone());

    let plugin = plugin
        .start(State {
            encoder,
            invoice_helper: invoice_helper.clone(),
            handler: Handler::new(invoice_helper, settler.clone()),
            settler,
        })
        .await?;

    let grpc_server = grpc::server::Server::new(
        &grpc_host,
        grpc_port,
        std::env::current_dir()?.join(utils::built_info::PKG_NAME),
    );

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

    Ok(())
}
