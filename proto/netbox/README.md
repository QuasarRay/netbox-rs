# Generated NetBox protobuf contracts

The `.proto` files in this directory are generated from the committed NetBox OpenAPI snapshot at `../../openapi/openapi.json` by the Rust generator at `../../src/bin/netbox_openapi_proto.rs`.

Do not edit generated `.proto` files manually.

Regenerate them from the repository root with:

```sh
cargo run --bin netbox_openapi_proto -- \
  --input openapi/openapi.json \
  --output-dir proto/netbox \
  --package netbox.v1
```

The manifest `.netbox-openapi-proto-manifest` tracks only generator-owned files. This README is intentionally not listed there.
