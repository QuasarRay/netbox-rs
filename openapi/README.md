# NetBox OpenAPI source snapshot

`openapi.json` is committed verbatim from NetBox's machine-readable REST API schema.

Initial provenance:

- repository: `netbox-community/netbox`
- path: `contrib/openapi.json`
- commit: `dbec965a5186a40971312c9331dfb9db46cc9183`
- OpenAPI-reported NetBox version: **4.7.1**

The final CI workflow verifies that this snapshot deterministically reproduces the committed protobuf contracts and can refresh both files from a selected upstream NetBox ref.
