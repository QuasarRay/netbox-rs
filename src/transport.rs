use bytes::{Buf, BufMut};
use serde::{Serialize, de::DeserializeOwned};
use std::{future::Future, io, marker::PhantomData};
use summer_grpc::tonic::{
    Code, Request, Response, Status,
    codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder},
    server::UnaryService,
};

#[derive(Clone, Copy, Debug)]
pub struct CborCodec<E, D>(PhantomData<fn() -> (E, D)>);

#[derive(Clone, Copy, Debug)]
pub struct CborEncoder<T>(PhantomData<fn() -> T>);

#[derive(Clone, Copy, Debug)]
pub struct CborDecoder<T>(PhantomData<fn() -> T>);

macro_rules! default_marker {
    ($($ty:ident),* => $name:ident<$($arg:ident),*>) => {
        impl<$($ty),*> Default for $name<$($arg),*> {
            fn default() -> Self { Self(PhantomData) }
        }
    };
}
default_marker!(E, D => CborCodec<E, D>);
default_marker!(T => CborEncoder<T>);
default_marker!(T => CborDecoder<T>);

impl<E, D> Codec for CborCodec<E, D>
where
    E: Serialize + Send + 'static,
    D: DeserializeOwned + Send + 'static,
{
    type Encode = E;
    type Decode = D;
    type Encoder = CborEncoder<E>;
    type Decoder = CborDecoder<D>;

    fn encoder(&mut self) -> Self::Encoder {
        CborEncoder(PhantomData)
    }

    fn decoder(&mut self) -> Self::Decoder {
        CborDecoder(PhantomData)
    }
}

struct BufWriter<'a, 'b>(&'a mut EncodeBuf<'b>);

impl io::Write for BufWriter<'_, '_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.put_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<T> Encoder for CborEncoder<T>
where
    T: Serialize,
{
    type Item = T;
    type Error = Status;

    fn encode(&mut self, item: T, dst: &mut EncodeBuf<'_>) -> Result<(), Self::Error> {
        ciborium::ser::into_writer(&item, BufWriter(dst))
            .map_err(|error| Status::internal(format!("CBOR encode failed: {error}")))
    }
}

impl<T> Decoder for CborDecoder<T>
where
    T: DeserializeOwned,
{
    type Item = T;
    type Error = Status;

    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Self::Item>, Self::Error> {
        let bytes = src.copy_to_bytes(src.remaining());
        ciborium::de::from_reader(bytes.as_ref())
            .map(Some)
            .map_err(|error| Status::invalid_argument(format!("CBOR decode failed: {error}")))
    }
}

pub fn status<E: crate::api::ApiError>(error: E) -> Status {
    let status = error.status();
    let code = match status {
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
    let mut details = Vec::new();
    match ciborium::ser::into_writer(&error, &mut details) {
        Ok(()) => Status::with_details(
            code,
            format!("NetBox API error {status}"),
            bytes::Bytes::from(details),
        ),
        Err(error) => Status::internal(format!("CBOR error encoding failed: {error}")),
    }
}

pub struct Unary<F>(pub F);

impl<F, Fut, Req, Res> UnaryService<Req> for Unary<F>
where
    F: FnMut(Request<Req>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Response<Res>, Status>> + Send + 'static,
{
    type Response = Res;
    type Future = Fut;

    fn call(&mut self, request: Request<Req>) -> Self::Future {
        (self.0)(request)
    }
}

#[macro_export]
#[doc(hidden)]
macro_rules! __netbox_grpc_service {
    (
        $vis:vis $server:ident {
            api: $api:path;
            name: $name:literal;
            $( $method:ident ( $req:ty ) -> $res:ty = $route:literal; )*
        }
    ) => {
        $vis struct $server<T> {
            inner: ::std::sync::Arc<T>,
        }

        impl<T> $server<T> {
            pub fn new(inner: T) -> Self {
                Self::from_arc(::std::sync::Arc::new(inner))
            }

            pub fn from_arc(inner: ::std::sync::Arc<T>) -> Self {
                Self { inner }
            }
        }

        impl<T> ::core::clone::Clone for $server<T> {
            fn clone(&self) -> Self {
                Self {
                    inner: ::std::sync::Arc::clone(&self.inner),
                }
            }
        }

        impl<T> ::summer_grpc::tonic::server::NamedService for $server<T> {
            const NAME: &'static str = $name;
        }

        impl<T, B> ::summer_grpc::tonic::codegen::Service<
            ::summer_grpc::tonic::codegen::http::Request<B>,
        > for $server<T>
        where
            T: $api,
            B: ::summer_grpc::tonic::codegen::Body + Send + 'static,
            B::Error: Into<::summer_grpc::tonic::codegen::StdError> + Send + 'static,
        {
            type Response = ::summer_grpc::tonic::codegen::http::Response<
                ::summer_grpc::tonic::body::Body,
            >;
            type Error = ::core::convert::Infallible;
            type Future = ::summer_grpc::tonic::codegen::BoxFuture<Self::Response, Self::Error>;

            fn poll_ready(
                &mut self,
                _cx: &mut ::core::task::Context<'_>,
            ) -> ::core::task::Poll<Result<(), Self::Error>> {
                ::core::task::Poll::Ready(Ok(()))
            }

            fn call(
                &mut self,
                req: ::summer_grpc::tonic::codegen::http::Request<B>,
            ) -> Self::Future {
                match req.uri().path() {
                    $(
                        $route => {
                            let inner = ::std::sync::Arc::clone(&self.inner);
                            ::std::boxed::Box::pin(async move {
                                let method = $crate::transport::Unary(
                                    move |request: ::summer_grpc::tonic::Request<$req>| {
                                        let inner = ::std::sync::Arc::clone(&inner);
                                        async move {
                                            inner
                                                .$method(request.into_inner())
                                                .await
                                                .map(::summer_grpc::tonic::Response::new)
                                                .map_err($crate::transport::status)
                                        }
                                    },
                                );
                                let mut grpc = ::summer_grpc::tonic::server::Grpc::new(
                                    $crate::transport::CborCodec::<$res, $req>::default(),
                                );
                                Ok(grpc.unary(method, req).await)
                            })
                        }
                    )*
                    _ => ::std::boxed::Box::pin(async move {
                        let mut response = ::summer_grpc::tonic::codegen::http::Response::new(
                            ::summer_grpc::tonic::body::Body::default(),
                        );
                        response.headers_mut().insert(
                            "grpc-status",
                            ::summer_grpc::tonic::codegen::http::HeaderValue::from_static("12"),
                        );
                        response.headers_mut().insert(
                            ::summer_grpc::tonic::codegen::http::header::CONTENT_TYPE,
                            ::summer_grpc::tonic::codegen::http::HeaderValue::from_static(
                                "application/grpc",
                            ),
                        );
                        Ok(response)
                    }),
                }
            }
        }
    };
}
