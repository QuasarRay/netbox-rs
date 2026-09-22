# NetBox OpenAPI source snapshot

`openapi.json` is committed verbatim from NetBox's machine-readable REST API schema at `contrib/openapi.json`.

See `SOURCE` for the exact upstream repository, commit SHA, source path, and OpenAPI-reported NetBox version for the currently committed snapshot.

Normal CI uses this committed file as the reproducible source of truth and verifies that it regenerates `proto/netbox/` byte-for-byte. The scheduled/manual refresh workflow resolves a selected upstream NetBox ref, replaces this snapshot, updates `SOURCE`, regenerates the protobuf contracts, compiles them with `protoc`, and opens or refreshes an update pull request.
