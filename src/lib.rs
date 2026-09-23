extern crate self as netbox_rs;

pub mod api;
pub mod implementation;
pub mod transport;

pub use netbox_rs_macros::{netbox_api, netbox_impl};

#[cfg(test)]
mod compile_tests {
    struct Status;

    impl crate::api::Status for Status {
        async fn status_retrieve(
            &self,
            _request: crate::api::models::RpcStatusRetrieveRequest,
        ) -> crate::api::Result<crate::api::models::RpcStatusRetrieveResponse> {
            unreachable!()
        }
    }

    #[test]
    fn ordinary_async_impl_builds_summer_service() {
        let _ = crate::implementation::status::StatusServer::new(Status);
    }
}
