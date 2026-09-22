use summer_grpc::tonic::{Code, Status};

impl From<crate::api::Error> for Status {
    fn from(error: crate::api::Error) -> Self {
        let code = match error.status {
            400 | 422 => Code::InvalidArgument,
            401 => Code::Unauthenticated,
            403 => Code::PermissionDenied,
            404 => Code::NotFound,
            409 => Code::AlreadyExists,
            429 => Code::ResourceExhausted,
            500 => Code::Internal,
            501 => Code::Unimplemented,
            502..=504 => Code::Unavailable,
            _ => Code::Unknown,
        };
        Status::new(code, error.message)
    }
}

netbox_rs_macros::netbox_impl!("openapi/openapi.json");
