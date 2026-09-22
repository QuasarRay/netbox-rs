# netbox-rs

NetBox's OpenAPI document is the source code. `netbox-rs` compiles it into a native Rust API and Summer/Tonic gRPC services; handwritten endpoint models and `.proto` domain contracts are unnecessary.

```text
openapi/openapi.json
        │
        ▼
netbox-rs-macros
  openapiv3 parser → normalized JSON Schema → Typify
        │                                │
        ├──────── native Rust models ────┤
        ├──────── async service traits   │
        └──────── Summer/Tonic adapters ─┘
                         │
                         ▼
                  CBOR over gRPC/HTTP2
```

## API / implementation split

The public API is deliberately tiny:

```rust
// src/api.rs
netbox_rs_macros::netbox_api!("openapi/openapi.json");
```

That one macro invocation generates all NetBox models, operation metadata, and per-domain async traits (`Dcim`, `Ipam`, `Circuits`, ...). The generated types are native Rust types; protobuf and Tonic do not appear in the domain API.

The transport side is equally small:

```rust
// src/implementation.rs
netbox_rs_macros::netbox_impl!("openapi/openapi.json");
```

It generates Summer-compatible `NamedService` adapters and routes each RPC through a custom Tonic CBOR codec. Register a domain implementation per NetBox service group:

```rust,ignore
implementation::dcim::register(&mut app, my_dcim);
implementation::ipam::register(&mut app, my_ipam);
```

`src/transport.rs` is generic infrastructure only: one CBOR codec, one unary adapter, and one declarative macro generate all concrete gRPC servers.

## Compiler design

`netbox-rs-macros` performs the work at compile time:

1. parse and validate the pinned OpenAPI document with `serde_json` + `openapiv3`;
2. normalize OpenAPI schema semantics into a JSON-Schema core (`$ref`, `nullable`, exclusive bounds, open objects);
3. synthesize request/response schemas from operations, parameters, bodies and 2xx responses;
4. let Typify lower that schema into native Rust structs/enums/newtypes;
5. emit service traits from `operationId` and service groups from `/api/<group>/...`;
6. emit Summer/Tonic server implementations with macro-generated routing.

The original OpenAPI remains the language-independent contract. Rust is a compiled implementation of that contract, not its source.

## Verification

`verus/schema_lowering.rs` contains the first Verus proof kernel. It proves the `nullable: true` normalization used by the compiler and the distinct missing/null/value states needed for PATCH semantics. The current proof boundary is intentionally explicit: the complete Typify/Ciborium/Tonic stack is not claimed to be formally verified yet.

The intended progression is to prove the normalization and codec kernel pass-by-pass while keeping parsing and code emission outside a small semantic core.

## Legacy Proto tooling

`src/bin/netbox_openapi_proto.rs`, `src/bin/netbox_openapi_fidelity.rs`, and `proto/netbox/` remain as compatibility/audit tooling. They are no longer required by the native Rust API architecture; OpenAPI is authoritative.
