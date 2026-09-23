# netbox-rs

NetBox's OpenAPI document is the source code. `netbox-rs` compiles it into native Rust models, a domain API, and Summer/Tonic services. No handwritten endpoint model layer or protobuf domain contract is required.

```text
openapi/openapi.json
        │
        ▼
 serde_json + openapiv3
        │
        ▼
 tiny NetBox codegen normalization
        │
        ▼
 Progenitor → Typify
        │
        ├── native Rust models
        ├── native service traits
        └── generated Summer/Tonic adapters
                         │
                         ▼
                    CBOR / gRPC
```

## API / implementation split

The authored public API is one macro invocation:

```rust
// src/api.rs
netbox_rs_macros::netbox_api!("openapi/openapi.json");
```

It generates every model, operation descriptor, and domain trait (`Dcim`, `Ipam`, `Circuits`, ...). Domain code sees native Rust only: request/response types plus methods returning `impl Future + Send`. This keeps Tonic, CBOR, protobuf, and transport details out of the API and avoids `async_trait` boxing.

Transport is generated independently:

```rust
// src/implementation.rs
netbox_rs_macros::netbox_impl!("openapi/openapi.json");
```

Each generated domain module exposes a Summer registration function:

```rust,ignore
implementation::dcim::register(&mut app, my_dcim);
implementation::ipam::register(&mut app, my_ipam);
```

`src/transport.rs` is the shared implementation kernel: one CBOR codec, one unary adapter, and one declarative macro generate all concrete Tonic `NamedService` implementations.

## Compiler

`netbox-rs-macros` does the repetitive work at compile time:

1. parse the pinned contract with `serde_json` and validate it with `openapiv3`;
2. normalize only NetBox/codegen edge cases while leaving the authoritative OpenAPI file untouched;
3. synthesize a request component from path/query parameters and request body for every operation;
4. synthesize a response component from successful responses;
5. delegate OpenAPI schema semantics and Rust type construction to Progenitor/Typify;
6. generate domain traits from `operationId`;
7. generate the Summer/Tonic routing layer from the same operation table.

The codegen view removes schema defaults (runtime REST behavior, not Rust type identity), drops enum members that cannot satisfy their declared primitive type, and preserves open objects with named fields. The original OpenAPI document remains the complete language-independent contract.

RPC names are deterministic from that contract: paths are grouped by the first segment after `/api/`, and each `operationId` determines the method name and generated request/response names. CBOR encodes the generated Serde data model over Tonic's codec-agnostic gRPC transport.

## Verification

`verus/schema_lowering.rs` is a small proof kernel for transformations owned by this repository. It proves the abstract equivalence between OpenAPI nullable values and the generated optional representation, proves that deleting enum members impossible under the declared primitive type cannot change the accepted-value set, and records the distinct missing/null/value states needed by partial-update semantics.

This is intentionally a narrow claim. `serde_json`, `openapiv3`, Progenitor/Typify, Ciborium, Tonic, Summer, and rustc remain in the trusted computing base. The next high-value proof target is the generated CBOR encode/decode boundary.

## Legacy proto tooling

`src/bin/netbox_openapi_proto.rs`, `src/bin/netbox_openapi_fidelity.rs`, and `proto/netbox/` remain as compatibility/audit tooling. They are not part of the native domain API architecture.
