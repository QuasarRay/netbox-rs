# Verification boundary

`schema_lowering.rs` is the first executable proof kernel for the OpenAPI compiler. It proves the semantic equivalence of the OpenAPI 3.0 `nullable: true` lowering used by the proc macro and records the three-state presence model required for PATCH semantics.

The production generator intentionally has a narrow trusted boundary: parsing is delegated to `serde_json`/`openapiv3`; JSON-Schema-to-Rust construction is delegated to Typify; `netbox-rs-macros` owns normalization, RPC synthesis, and service generation. The next proof targets are `$ref` preservation, `additionalProperties` flattening, union lowering, and CBOR encode/decode round trips.

Run this file with a Verus release that supports the repository Rust toolchain:

```sh
verus verus/schema_lowering.rs
```

This proof does **not** claim that Typify, Ciborium, Tonic, or the complete OpenAPI compiler is formally verified yet.
