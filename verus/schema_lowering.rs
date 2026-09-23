use vstd::prelude::*;

verus! {

#[derive(PartialEq, Eq)]
pub enum ValueKind { Null, Value }

pub open spec fn openapi_nullable(base: bool, kind: ValueKind) -> bool {
    match kind { ValueKind::Null => true, ValueKind::Value => base }
}

pub open spec fn rust_option_accepts(base: bool, kind: ValueKind) -> bool {
    match kind { ValueKind::Null => true, ValueKind::Value => base }
}

pub proof fn nullable_representation_preserves_acceptance(base: bool, kind: ValueKind)
    ensures openapi_nullable(base, kind) == rust_option_accepts(base, kind)
{}

pub open spec fn typed_enum_accepts(type_matches: bool, listed: bool) -> bool {
    type_matches && listed
}

pub open spec fn filtered_enum_lists(type_matches: bool, listed: bool) -> bool {
    listed && type_matches
}

pub proof fn dropping_type_impossible_enum_members_preserves_acceptance(
    type_matches: bool,
    listed: bool,
)
    ensures
        typed_enum_accepts(type_matches, listed)
            == typed_enum_accepts(type_matches, filtered_enum_lists(type_matches, listed))
{}

#[derive(PartialEq, Eq)]
pub enum Presence<T> { Missing, Null, Value(T) }

pub proof fn presence_states_are_distinct<T>(x: T)
    ensures
        Presence::Missing != Presence::Null,
        Presence::Missing != Presence::Value(x),
        Presence::Null != Presence::Value(x),
{}

}
