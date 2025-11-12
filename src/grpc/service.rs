use crate::database::helpers::invoice_helper::InvoiceHelper;
use crate::database::model::{InvoiceInsertable, InvoiceState};
use crate::encoder::{InvoiceBuilder, InvoiceDescription, InvoiceEncoder};
use crate::grpc::service::hold::hold_server::Hold;
use crate::grpc::service::hold::invoice_request::Description;
use crate::grpc::service::hold::list_request::Constraint;
use crate::grpc::service::hold::{
    CancelRequest, CancelResponse, CleanRequest, CleanResponse, GetInfoRequest, GetInfoResponse,
    HookAction, InjectRequest, InjectResponse, InvoiceRequest, InvoiceResponse, ListRequest,
    ListResponse, OnionMessage, OnionMessageResponse, SettleRequest, SettleResponse,
    TrackAllRequest, TrackAllResponse, TrackRequest, TrackResponse,
};
use crate::grpc::transformers::{transform_invoice_state, transform_route_hints};
use crate::invoice::Invoice;
use crate::messenger::Messenger;
use crate::settler::Settler;
use bitcoin::hashes::{Hash, sha256};
use std::collections::HashMap;
use std::pin::Pin;
use std::str::FromStr;
use tokio::sync::mpsc;
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;
use tonic::codegen::tokio_stream::{Stream, StreamExt};
use tonic::{Code, Request, Response, Status, Streaming, async_trait};
use tracing::instrument;
use tracing::{debug, error, warn};

pub mod hold {
    tonic::include_proto!("hold");
}

#[cfg(feature = "otel")]
struct MetadataMap<'a>(&'a tonic::metadata::MetadataMap);

#[cfg(feature = "otel")]
impl opentelemetry::propagation::Extractor for MetadataMap<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|metadata| metadata.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0
            .keys()
            .map(|key| match key {
                tonic::metadata::KeyRef::Ascii(v) => v.as_str(),
                tonic::metadata::KeyRef::Binary(v) => v.as_str(),
            })
            .collect::<Vec<_>>()
    }
}

fn extract_parent_context<T>(request: &Request<T>) {
    #[cfg(feature = "otel")]
    {
        use tracing_opentelemetry::OpenTelemetrySpanExt;

        let parent_cx = opentelemetry::global::get_text_map_propagator(|prop| {
            prop.extract(&MetadataMap(request.metadata()))
        });
        let _ = tracing::Span::current().set_parent(parent_cx);
    }
}

pub struct HoldService<T, E> {
    our_id: [u8; 33],
    encoder: E,
    invoice_helper: T,
    settler: Settler<T>,
    messenger: Messenger,
}

impl<T, E> HoldService<T, E>
where
    T: InvoiceHelper + Send + Sync + Clone + 'static,
    E: InvoiceEncoder + Send + Sync + Clone + 'static,
{
    pub fn new(
        our_id: [u8; 33],
        invoice_helper: T,
        encoder: E,
        settler: Settler<T>,
        messenger: Messenger,
    ) -> Self {
        HoldService {
            our_id,
            encoder,
            settler,
            messenger,
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
    #[instrument(name = "grpc::get_info", skip_all)]
    async fn get_info(
        &self,
        request: Request<GetInfoRequest>,
    ) -> Result<Response<GetInfoResponse>, Status> {
        extract_parent_context(&request);

        Ok(Response::new(GetInfoResponse {
            version: crate::utils::built_info::PKG_VERSION.to_string(),
        }))
    }

    #[instrument(name = "grpc::invoice", skip_all)]
    async fn invoice(
        &self,
        request: Request<InvoiceRequest>,
    ) -> Result<Response<InvoiceResponse>, Status> {
        extract_parent_context(&request);

        let params = request.into_inner();

        let route_hints = match transform_route_hints(params.routing_hints) {
            Ok(hints) => hints,
            Err(err) => {
                return Err(Status::new(
                    Code::InvalidArgument,
                    format!("invalid routing hint: {err}"),
                ));
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
                    format!("could not encode invoice: {err}"),
                ));
            }
        };

        if let Err(err) = self.invoice_helper.insert(&InvoiceInsertable {
            invoice: invoice.clone(),
            payment_hash: params.payment_hash.clone(),
            state: InvoiceState::Unpaid.into(),
            min_cltv: params.min_final_cltv_expiry.map(|cltv| cltv as i32),
        }) {
            return Err(Status::new(
                Code::Internal,
                format!("could not save invoice: {err}"),
            ));
        }

        self.settler
            .new_invoice(invoice.clone(), params.payment_hash, params.amount_msat);

        Ok(Response::new(InvoiceResponse { bolt11: invoice }))
    }

    #[instrument(name = "grpc::inject", skip_all)]
    async fn inject(
        &self,
        request: Request<InjectRequest>,
    ) -> Result<Response<InjectResponse>, Status> {
        extract_parent_context(&request);

        let params = request.into_inner();

        let invoice = Invoice::from_str(&params.invoice)
            .map_err(|err| Status::new(Code::InvalidArgument, format!("invalid invoice: {err}")))?;

        // Sanity check that the invoice can go through us
        if !invoice.related_to_node(self.our_id) {
            return Err(Status::new(
                Code::InvalidArgument,
                "invoice is not related to us".to_string(),
            ));
        }

        self.invoice_helper
            .insert(&InvoiceInsertable {
                invoice: params.invoice.clone(),
                payment_hash: invoice.payment_hash().to_vec(),
                state: InvoiceState::Unpaid.into(),
                min_cltv: params.min_cltv_expiry.map(|cltv| cltv as i32),
            })
            .map_err(|err| Status::new(Code::Internal, format!("could not save invoice: {err}")))?;

        self.settler.new_invoice(
            params.invoice,
            invoice.payment_hash().to_vec(),
            invoice.amount_milli_satoshis().unwrap_or(0),
        );

        Ok(Response::new(InjectResponse {}))
    }

    #[instrument(name = "grpc::list", skip_all)]
    async fn list(&self, request: Request<ListRequest>) -> Result<Response<ListResponse>, Status> {
        extract_parent_context(&request);

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
                format!("could not fetch invoices: {err}"),
            )),
        }
    }

    #[instrument(name = "grpc::settle", skip_all)]
    async fn settle(
        &self,
        request: Request<SettleRequest>,
    ) -> Result<Response<SettleResponse>, Status> {
        extract_parent_context(&request);

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
                format!("could not settle invoice: {err}"),
            ));
        };

        Ok(Response::new(SettleResponse {}))
    }

    #[instrument(name = "grpc::cancel", skip_all)]
    async fn cancel(
        &self,
        request: Request<CancelRequest>,
    ) -> Result<Response<CancelResponse>, Status> {
        extract_parent_context(&request);

        if let Err(err) = self
            .settler
            .clone()
            .cancel(&request.into_inner().payment_hash)
            .await
        {
            return Err(Status::new(
                Code::Internal,
                format!("could not cancel invoice: {err}"),
            ));
        };

        Ok(Response::new(CancelResponse {}))
    }

    #[instrument(name = "grpc::clean", skip_all)]
    async fn clean(
        &self,
        request: Request<CleanRequest>,
    ) -> Result<Response<CleanResponse>, Status> {
        extract_parent_context(&request);

        let params = request.into_inner();
        match self.invoice_helper.clean_cancelled(params.age) {
            Ok(deleted) => Ok(Response::new(CleanResponse {
                cleaned: deleted as u64,
            })),
            Err(err) => Err(Status::new(
                Code::Internal,
                format!("could not clean invoices: {err}"),
            )),
        }
    }

    type TrackStream = Pin<Box<dyn Stream<Item = Result<TrackResponse, Status>> + Send>>;

    #[instrument(name = "grpc::track", skip_all)]
    async fn track(
        &self,
        request: Request<TrackRequest>,
    ) -> Result<Response<Self::TrackStream>, Status> {
        extract_parent_context(&request);

        let params = request.into_inner();
        let (tx, rx) = mpsc::channel(16);

        let mut initial_state = None;
        let mut state_rx = self.settler.state_rx();

        match self
            .invoice_helper
            .get_by_payment_hash(&params.payment_hash)
        {
            Ok(res) => {
                if let Some(res) = res {
                    match InvoiceState::try_from(res.invoice.state.as_str()) {
                        Ok(state) => {
                            initial_state = Some(state);
                            if let Err(err) = tx
                                .send(Ok(TrackResponse {
                                    state: transform_invoice_state(state),
                                }))
                                .await
                            {
                                error!("Could not send invoice state update: {err}");
                                return Err(Status::new(
                                    Code::Internal,
                                    format!("could not send initial invoice state: {err}"),
                                ));
                            }
                        }
                        Err(err) => {
                            return Err(Status::new(
                                Code::Internal,
                                format!("could not transform invoice state: {err}"),
                            ));
                        }
                    }
                }
            }
            Err(err) => {
                return Err(Status::new(
                    Code::Internal,
                    format!("could not fetch invoice state from database: {err}"),
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

                        // Do not send the initial state twice
                        if let Some(initial_state) = initial_state
                            && initial_state == update.state
                        {
                            continue;
                        }

                        if let Err(err) = tx
                            .send(Ok(TrackResponse {
                                state: transform_invoice_state(update.state),
                            }))
                            .await
                        {
                            debug!("Could not send invoice state update: {err}");
                            break;
                        };

                        if update.state.is_final() {
                            break;
                        }
                    }
                    Err(err) => {
                        error!("Waiting for invoice state updates failed: {err}");
                        break;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    type TrackAllStream = Pin<Box<dyn Stream<Item = Result<TrackAllResponse, Status>> + Send>>;

    #[instrument(name = "grpc::track_all", skip_all)]
    async fn track_all(
        &self,
        request: Request<TrackAllRequest>,
    ) -> Result<Response<Self::TrackAllStream>, Status> {
        extract_parent_context(&request);

        let params = request.into_inner();

        let (tx, rx) = mpsc::channel(128);

        let mut initial_states = HashMap::new();
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
                        error!("{err}");
                        let _ = tx.send(Err(Status::new(Code::Internal, err))).await;
                        return;
                    }
                };

                let state = match InvoiceState::try_from(invoice.invoice.state.as_str()) {
                    Ok(state) => state,
                    Err(err) => {
                        let err = format!(
                            "Could not parse state of invoice {}: {}",
                            hex::encode(&hash),
                            err
                        );
                        error!("{err}");
                        let _ = tx.send(Err(Status::new(Code::Internal, err))).await;
                        return;
                    }
                };
                initial_states.insert(hash, state);

                if let Err(err) = tx
                    .send(Ok(TrackAllResponse {
                        bolt11: invoice.invoice.invoice,
                        state: transform_invoice_state(state),
                        payment_hash: invoice.invoice.payment_hash,
                    }))
                    .await
                {
                    error!("Could not send invoice state: {err}");
                    return;
                };
            }

            loop {
                match state_rx.recv().await {
                    Ok(update) => {
                        // Do not send the initial state twice
                        if let Some(initial_state) = initial_states.get(&update.payment_hash)
                            && initial_state == &update.state
                        {
                            continue;
                        }

                        if let Err(err) = tx
                            .send(Ok(TrackAllResponse {
                                bolt11: update.invoice,
                                payment_hash: update.payment_hash,
                                state: transform_invoice_state(update.state),
                            }))
                            .await
                        {
                            debug!("Could not send all invoices state update: {err}");
                            break;
                        };
                    }
                    Err(err) => {
                        error!("Waiting for all invoices state updates failed: {err}");
                        break;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    type OnionMessagesStream = Pin<Box<dyn Stream<Item = Result<OnionMessage, Status>> + Send>>;

    #[instrument(name = "grpc::onion_messages", skip_all)]
    async fn onion_messages(
        &self,
        response: Request<Streaming<OnionMessageResponse>>,
    ) -> Result<Response<Self::OnionMessagesStream>, Status> {
        extract_parent_context(&response);

        let (tx, rx) = mpsc::channel(128);
        let mut onion_rx = self.messenger.subscribe();

        {
            let messenger = self.messenger.clone();
            let mut in_stream = response.into_inner();
            tokio::spawn(async move {
                while let Some(res) = in_stream.next().await {
                    match res {
                        Ok(res) => {
                            messenger.send_response(
                                res.id,
                                if res.action == HookAction::Continue as i32 {
                                    crate::hooks::onion_message::OnionMessageResponse::Continue
                                } else {
                                    crate::hooks::onion_message::OnionMessageResponse::Resolve
                                },
                            );
                        }
                        Err(err) => {
                            error!("Onion message response error: {err}");
                            break;
                        }
                    }
                }
            });
        }

        tokio::spawn(async move {
            loop {
                match onion_rx.recv().await {
                    Ok(msg) => {
                        let msg: OnionMessage = match msg.try_into() {
                            Ok(msg) => msg,
                            Err(err) => {
                                error!("Failed to convert onion message: {err}");
                                break;
                            }
                        };

                        if let Err(err) = tx.send(Ok(msg)).await {
                            error!("Failed to send onion message: {err}");
                            break;
                        }
                    }
                    Err(err) => {
                        error!("Waiting for onion messages failed: {err}");
                        break;
                    }
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}
