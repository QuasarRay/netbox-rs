# NetBox protobuf contracts

These protobuf contracts are generated from NetBox's committed machine-readable OpenAPI schema:

- upstream: `netbox-community/netbox`
- source: `contrib/openapi.json`
- OpenAPI-reported NetBox version: **4.7.1**
- protobuf package: `netbox.v1`

The generated files model the public REST API contract: component schemas become protobuf messages/enums and REST operations become app-grouped gRPC service methods.

OpenAPI is treated as an API contract, not as NetBox's database schema. NetBox models and migrations remain authoritative for persistence semantics.

Generated enum identifiers include a stable hash of the exact upstream value to avoid protobuf canonical-name collisions (for example interface speeds such as `2.5gbase-t` versus `25gbase-t`).

Do not hand-edit the generated `.proto` files.
