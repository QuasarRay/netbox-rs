# NetBox OpenAPI source snapshot

`openapi.json` is committed verbatim from NetBox's machine-readable REST API schema at `contrib/openapi.json`.

See `SOURCE` for the exact upstream repository, commit SHA, source path, and OpenAPI-reported NetBox version for the currently committed snapshot.

The native Rust API consumes this committed OpenAPI snapshot directly at compile time through `netbox-rs-macros`. No generated protobuf contract is required.
