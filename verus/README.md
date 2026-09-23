# Verification boundary

`schema_lowering.rs` is the proof kernel for semantics owned by this repository rather than semantics delegated to the OpenAPI ecosystem.

It currently proves:

- OpenAPI nullable acceptance is equivalent to the optional value model used for native Rust generation;
- removing an enum member that cannot satisfy the schema's declared primitive type does not change the schema's accepted-value set;
- missing, explicit null, and concrete value remain distinct states for partial-update reasoning.

Production parsing is delegated to `serde_json`/`openapiv3`, and general OpenAPI-to-Rust lowering is delegated to Progenitor/Typify. The proc macro owns NetBox-specific codegen normalization, request/response synthesis, operation grouping, and service generation. Ciborium/Tonic/Summer provide the transport implementation.

Run the proof with a compatible Verus release:

```sh
verus verus/schema_lowering.rs
```

The next useful proof target is the codec boundary: generated value → CBOR → generated value. This directory does **not** claim that Progenitor, Typify, Ciborium, Tonic, Summer, rustc, or the complete compiler pipeline is formally verified.
