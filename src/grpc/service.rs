use crate::grpc::service::hold::hold_server::Hold;
use crate::grpc::service::hold::{GetInfoRequest, GetInfoResponse};
use tonic::{async_trait, Request, Response, Status};

pub mod hold {
    tonic::include_proto!("hold");
}

pub struct HoldService {}

#[async_trait]
impl Hold for HoldService {
    async fn get_info(
        &self,
        _: Request<GetInfoRequest>,
    ) -> Result<Response<GetInfoResponse>, Status> {
        Ok(Response::new(GetInfoResponse {
            version: crate::utils::built_info::PKG_VERSION.to_string(),
        }))
    }
}
