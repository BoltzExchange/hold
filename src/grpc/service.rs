use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::database::model::{InvoiceInsertable, InvoiceState};
use crate::encoder::{InvoiceBuilder, InvoiceDescription, InvoiceEncoder};
use crate::grpc::service::hold::hold_server::Hold;
use crate::grpc::service::hold::invoice_request::Description;
use crate::grpc::service::hold::list_request::Constraint;
use crate::grpc::service::hold::{
    CancelRequest, CancelResponse, GetInfoRequest, GetInfoResponse, InvoiceRequest,
    InvoiceResponse, ListRequest, ListResponse, SettleRequest, SettleResponse, TrackAllRequest,
    TrackAllResponse, TrackRequest, TrackResponse,
};
use crate::grpc::transformers::{transform_invoice_state, transform_route_hints};
use crate::settler::Settler;
use bitcoin::hashes::{sha256, Hash};
use log::{debug, error, warn};
use std::pin::Pin;
use tokio::sync::mpsc;
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;
use tonic::codegen::tokio_stream::Stream;
use tonic::{async_trait, Code, Request, Response, Status};

pub mod hold {
    tonic::include_proto!("hold");
}

pub struct HoldService<T, E> {
    encoder: E,
    invoice_helper: T,
    settler: Settler<T>,
}

impl<T, E> HoldService<T, E>
where
    T: InvoiceHelper + Send + Sync + Clone + 'static,
    E: InvoiceEncoder + Send + Sync + Clone + 'static,
{
    pub fn new(invoice_helper: T, encoder: E, settler: Settler<T>) -> Self {
        HoldService {
            encoder,
            settler,
            invoice_helper,
        }
    }
}

#[async_trait]
impl<T, E> Hold for HoldService<T, E>
where
    T: InvoiceHelper + Send + Sync + Clone + 'static,
    E: InvoiceEncoder + Send + Sync + Clone + 'static,
{
    async fn get_info(
        &self,
        _: Request<GetInfoRequest>,
    ) -> Result<Response<GetInfoResponse>, Status> {
        Ok(Response::new(GetInfoResponse {
            version: crate::utils::built_info::PKG_VERSION.to_string(),
        }))
    }

    async fn invoice(
        &self,
        request: Request<InvoiceRequest>,
    ) -> Result<Response<InvoiceResponse>, Status> {
        let params = request.into_inner();

        let route_hints = match transform_route_hints(params.routing_hints) {
            Ok(hints) => hints,
            Err(err) => {
                return Err(Status::new(
                    Code::InvalidArgument,
                    format!("invalid routing hint: {}", err),
                ))
            }
        };

        let mut builder = InvoiceBuilder::new(&params.payment_hash)
            .amount_msat(params.amount_msat)
            .route_hints(route_hints);

        if let Some(description) = params.description {
            builder = builder.description(match description {
                Description::Memo(memo) => InvoiceDescription::Description(memo),
                Description::Hash(hash) => InvoiceDescription::Hash(hash),
            });
        }

        if let Some(expiry) = params.expiry {
            builder = builder.expiry(expiry);
        }

        if let Some(delta) = params.min_final_cltv_expiry {
            builder = builder.min_final_cltv_expiry_delta(delta);
        }

        let invoice = match self.encoder.encode(builder).await {
            Ok(invoice) => invoice,
            Err(err) => {
                return Err(Status::new(
                    Code::Internal,
                    format!("could not encode invoice: {}", err),
                ))
            }
        };

        if let Err(err) = self.invoice_helper.insert(&InvoiceInsertable {
            bolt11: invoice.clone(),
            payment_hash: params.payment_hash.clone(),
            state: InvoiceState::Unpaid.into(),
        }) {
            return Err(Status::new(
                Code::Internal,
                format!("could not save invoice: {}", err),
            ));
        }

        self.settler
            .new_invoice(invoice.clone(), params.payment_hash, params.amount_msat);

        Ok(Response::new(InvoiceResponse { bolt11: invoice }))
    }

    async fn list(&self, request: Request<ListRequest>) -> Result<Response<ListResponse>, Status> {
        let params = request.into_inner();
        let invoices = match params.constraint {
            Some(constraint) => match constraint {
                Constraint::PaymentHash(payment_hash) => {
                    match self.invoice_helper.get_by_payment_hash(&payment_hash) {
                        Ok(invoice) => match invoice {
                            Some(invoice) => Ok(vec![invoice]),
                            None => Ok(Vec::new()),
                        },
                        Err(err) => Err(err),
                    }
                }
                Constraint::Pagination(pagination) => self
                    .invoice_helper
                    .get_paginated(pagination.index_start, pagination.limit),
            },
            None => self.invoice_helper.get_all(),
        };

        match invoices {
            Ok(invoices) => Ok(Response::new(ListResponse {
                invoices: invoices.into_iter().map(|invoice| invoice.into()).collect(),
            })),
            Err(err) => Err(Status::new(
                Code::Internal,
                format!("could not fetch invoices: {}", err),
            )),
        }
    }

    async fn settle(
        &self,
        request: Request<SettleRequest>,
    ) -> Result<Response<SettleResponse>, Status> {
        let preimage = request.into_inner().payment_preimage;
        let payment_hash: sha256::Hash = Hash::hash(&preimage);

        if let Err(err) = self
            .settler
            .clone()
            .settle(&payment_hash[..].to_vec(), preimage.as_ref())
            .await
        {
            return Err(Status::new(
                Code::Internal,
                format!("could not settle invoice: {}", err),
            ));
        };

        Ok(Response::new(SettleResponse {}))
    }

    async fn cancel(
        &self,
        request: Request<CancelRequest>,
    ) -> Result<Response<CancelResponse>, Status> {
        if let Err(err) = self
            .settler
            .clone()
            .cancel(&request.into_inner().payment_hash)
            .await
        {
            return Err(Status::new(
                Code::Internal,
                format!("could not cancel invoice: {}", err),
            ));
        };

        Ok(Response::new(CancelResponse {}))
    }

    type TrackStream = Pin<Box<dyn Stream<Item = Result<TrackResponse, Status>> + Send>>;

    async fn track(
        &self,
        request: Request<TrackRequest>,
    ) -> Result<Response<Self::TrackStream>, Status> {
        let params = request.into_inner();
        let (tx, rx) = mpsc::channel(16);

        let mut state_rx = self.settler.state_rx();

        match self
            .invoice_helper
            .get_by_payment_hash(&params.payment_hash)
        {
            Ok(res) => {
                if let Some(res) = res {
                    if let Ok(state) = InvoiceState::try_from(res.invoice.state.as_str()) {
                        if let Err(err) = tx
                            .send(Ok(TrackResponse {
                                state: transform_invoice_state(state),
                            }))
                            .await
                        {
                            error!("Could not send invoice state update: {}", err);
                            return Err(Status::new(
                                Code::Internal,
                                format!("could not send initial invoice state: {}", err),
                            ));
                        }
                    }
                }
            }
            Err(err) => {
                return Err(Status::new(
                    Code::Internal,
                    format!("could not fetch invoice state from database: {}", err),
                ));
            }
        };

        tokio::spawn(async move {
            loop {
                match state_rx.recv().await {
                    Ok(update) => {
                        if !update.payment_hash.eq(&params.payment_hash) {
                            continue;
                        }

                        if let Err(err) = tx
                            .send(Ok(TrackResponse {
                                state: transform_invoice_state(update.state),
                            }))
                            .await
                        {
                            debug!("Could not send invoice state update: {}", err);
                            break;
                        };

                        if update.state.is_final() {
                            break;
                        }
                    }
                    Err(err) => {
                        error!("Waiting for invoice state updates failed: {}", err);
                        break;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    type TrackAllStream = Pin<Box<dyn Stream<Item = Result<TrackAllResponse, Status>> + Send>>;

    async fn track_all(
        &self,
        request: Request<TrackAllRequest>,
    ) -> Result<Response<Self::TrackAllStream>, Status> {
        let params = request.into_inner();

        let (tx, rx) = mpsc::channel(128);

        let invoice_helper = self.invoice_helper.clone();
        let mut state_rx = self.settler.state_rx();

        tokio::spawn(async move {
            for hash in params.payment_hashes {
                let invoice = match invoice_helper.get_by_payment_hash(&hash) {
                    Ok(invoice) => match invoice {
                        Some(invoice) => invoice,
                        None => {
                            warn!(
                                "Could not find invoice with payment hash: {}",
                                hex::encode(&hash)
                            );
                            continue;
                        }
                    },
                    Err(err) => {
                        let err = format!(
                            "Could not get invoice with payment hash {}: {}",
                            hex::encode(&hash),
                            err
                        );
                        error!("{}", err);
                        let _ = tx.send(Err(Status::new(Code::Internal, err))).await;
                        return;
                    }
                };

                let state = transform_invoice_state(
                    match InvoiceState::try_from(invoice.invoice.state.as_str()) {
                        Ok(state) => state,
                        Err(err) => {
                            let err = format!(
                                "Could not parse state of invoice {}: {}",
                                hex::encode(&hash),
                                err
                            );
                            error!("{}", err);
                            let _ = tx.send(Err(Status::new(Code::Internal, err))).await;
                            return;
                        }
                    },
                );

                if let Err(err) = tx
                    .send(Ok(TrackAllResponse {
                        state,
                        bolt11: invoice.invoice.bolt11,
                        payment_hash: invoice.invoice.payment_hash,
                    }))
                    .await
                {
                    error!("Could not send invoice state: {}", err);
                    return;
                };
            }

            loop {
                match state_rx.recv().await {
                    Ok(update) => {
                        if let Err(err) = tx
                            .send(Ok(TrackAllResponse {
                                bolt11: update.bolt11,
                                payment_hash: update.payment_hash,
                                state: transform_invoice_state(update.state),
                            }))
                            .await
                        {
                            debug!("Could not send all invoices state update: {}", err);
                            break;
                        };
                    }
                    Err(err) => {
                        error!("Waiting for all invoices state updates failed: {}", err);
                        break;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}
