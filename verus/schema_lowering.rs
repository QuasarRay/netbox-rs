use vstd::prelude::*;

verus! {

#[derive(PartialEq, Eq)]
pub enum ValueKind { Null, Value }

/// Semantic kernel used by the OpenAPI `nullable: true` desugaring in the
/// proc-macro compiler. `base` means the non-null schema accepts the value.
pub open spec fn openapi_nullable(base: bool, kind: ValueKind) -> bool {
    match kind { ValueKind::Null => true, ValueKind::Value => base }
}

/// JSON-Schema core emitted by the compiler: `anyOf: [base, {type:null}]`.
pub open spec fn lowered_any_of(base: bool, kind: ValueKind) -> bool {
    base && kind == ValueKind::Value || kind == ValueKind::Null
}

pub proof fn nullable_lowering_preserves_acceptance(base: bool, kind: ValueKind)
    ensures openapi_nullable(base, kind) == lowered_any_of(base, kind)
{}

#[derive(PartialEq, Eq)]
pub enum Presence<T> { Missing, Null, Value(T) }

/// The generated Rust representation preserves the three JSON states needed
/// by PATCH-like contracts: omitted, explicit null, and a concrete value.
pub proof fn presence_states_are_distinct<T>(x: T)
    ensures
        Presence::Missing != Presence::Null,
        Presence::Missing != Presence::Value(x),
        Presence::Null != Presence::Value(x),
{}

}
