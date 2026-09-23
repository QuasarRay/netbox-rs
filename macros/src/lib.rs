use heck::{ToPascalCase, ToSnakeCase};
use openapiv3::OpenAPI;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use progenitor::{GenerationSettings, Generator};
use quote::{format_ident, quote};
use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, env, fs, path::PathBuf};
use syn::LitStr;

#[proc_macro]
pub fn netbox_api(input: TokenStream) -> TokenStream {
    expand(input, api).unwrap_or_else(error).into()
}

#[proc_macro]
pub fn netbox_impl(input: TokenStream) -> TokenStream {
    expand(input, implementation).unwrap_or_else(error).into()
}

fn expand(input: TokenStream, f: fn(&Doc) -> Result<Tokens, String>) -> Result<Tokens, String> {
    let path = syn::parse::<LitStr>(input)
        .map_err(|e| e.to_string())?
        .value();
    let file =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?).join(&path);
    let source = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let value: Value = serde_json::from_str(&source).map_err(|e| e.to_string())?;
    serde_json::from_value::<OpenAPI>(value.clone())
        .map_err(|e| format!("invalid OpenAPI: {e}"))?;
    let doc = Doc::new(value)?;
    let body = f(&doc)?;
    let path = LitStr::new(&path, proc_macro2::Span::call_site());
    Ok(quote! {
        const _: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #path));
        #body
    })
}

fn error(e: String) -> Tokens {
    let e = LitStr::new(&e, proc_macro2::Span::call_site());
    quote!(compile_error!(#e);)
}

#[derive(Clone)]
enum Model {
    Named(String),
    Unit,
}

#[derive(Clone)]
struct Failure {
    status: u16,
    body: Model,
}

#[derive(Clone)]
struct Op {
    id: String,
    group: String,
    method: String,
    path: String,
    request: String,
    request_ty: Model,
    response: String,
    response_ty: Model,
    failures: Vec<Failure>,
}

struct Doc {
    raw: Value,
    ops: Vec<Op>,
    types: OpenAPI,
}

impl Doc {
    fn new(raw: Value) -> Result<Self, String> {
        let mut typed = raw.clone();
        let schemas = typed
            .pointer_mut("/components/schemas")
            .and_then(Value::as_object_mut)
            .ok_or("OpenAPI components.schemas missing")?;

        for schema in schemas.values_mut() {
            *schema = codegen_schema(schema.take());
        }

        let mut pool = SchemaPool::new(schemas)?;
        let mut ops = Vec::new();
        let paths = raw
            .get("paths")
            .and_then(Value::as_object)
            .ok_or("OpenAPI paths missing")?;
        for (path, item) in paths {
            for method in [
                "get", "post", "put", "patch", "delete", "options", "head", "trace",
            ] {
                let Some(op) = item.get(method).filter(|v| v.is_object()) else {
                    continue;
                };
                let id = op
                    .get("operationId")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| fallback_id(method, path));
                let stem = pascal(&id);
                let request = format!("Rpc{stem}Request");
                let response = format!("Rpc{stem}Response");
                let request_ty = pool.intern(request_schema(&raw, item, op)?)?;
                let response_ty = response_type(&raw, op, &mut pool)?;
                let failures = failure_schemas(&raw, op, &mut pool)?;
                ops.push(Op {
                    id,
                    group: group(path),
                    method: method.to_ascii_uppercase(),
                    path: path.clone(),
                    request,
                    request_ty,
                    response,
                    response_ty,
                    failures,
                });
            }
        }
        ops.sort_by(|a, b| (&a.group, &a.id).cmp(&(&b.group, &b.id)));
        drop(pool);

        // Progenitor's OpenAPI -> Typify conversion is the type compiler.
        // Paths are irrelevant here: synthesized RPC shapes are components.
        typed["paths"] = json!({});
        let types = serde_json::from_value::<OpenAPI>(typed)
            .map_err(|e| format!("generated type OpenAPI invalid: {e}"))?;

        Ok(Self { raw, ops, types })
    }
}
fn api(doc: &Doc) -> Result<Tokens, String> {
    let settings = GenerationSettings::default();
    let mut generator = Generator::new(&settings);
    generator
        .generate_tokens(&doc.types)
        .map_err(|e| format!("OpenAPI Rust type generation failed: {e}"))?;
    let models = generator.get_type_space().to_stream();
    let aliases = doc.ops.iter().flat_map(|op| {
        [
            model_alias(&op.request, &op.request_ty),
            model_alias(&op.response, &op.response_ty),
        ]
    });

    let services = grouped(&doc.ops).into_iter().map(|(group, ops)| {
        let trait_name = format_ident!("{}", pascal(&group));
        let methods = ops.iter().map(|op| {
            let name = format_ident!("{}", snake(&op.id));
            let req = format_ident!("{}", op.request);
            let res = format_ident!("{}", op.response);
            let err = if op.failures.is_empty() {
                format_ident!("Never")
            } else {
                format_ident!("Rpc{}Error", pascal(&op.id))
            };
            quote!(
                fn #name(&self, request: models::#req)
                    -> impl ::core::future::Future<Output = Result<models::#res, #err>> + Send;
            )
        });
        quote! {
            pub trait #trait_name: Send + Sync + 'static { #(#methods)* }
        }
    });

    let errors = doc
        .ops
        .iter()
        .filter(|op| !op.failures.is_empty())
        .map(|op| {
            let error = format_ident!("Rpc{}Error", pascal(&op.id));
            let variants = op.failures.iter().map(|failure| {
                let variant = format_ident!("Status{}", failure.status);
                let rename = failure.status.to_string();
                match &failure.body {
                    Model::Named(body) => {
                        let body = format_ident!("{}", body);
                        quote!(
                            #[serde(rename = #rename)]
                            #variant(models::#body)
                        )
                    }
                    Model::Unit => quote!(
                        #[serde(rename = #rename)]
                        #variant
                    ),
                }
            });
            let statuses = op.failures.iter().map(|failure| {
                let variant = format_ident!("Status{}", failure.status);
                let status = failure.status;
                match &failure.body {
                    Model::Named(_) => quote!(Self::#variant(_) => #status),
                    Model::Unit => quote!(Self::#variant => #status),
                }
            });
            quote! {
                #[derive(Debug, ::serde::Serialize, ::serde::Deserialize)]
                #[serde(tag = "status", content = "body")]
                pub enum #error { #(#variants),* }

                impl ApiError for #error {
                    fn status(&self) -> u16 {
                        match self { #(#statuses),* }
                    }
                }
            }
        });

    let metadata = doc.ops.iter().map(|op| {
        let (id, group, method, path) = (&op.id, &op.group, &op.method, &op.path);
        quote!(Operation { id: #id, service: #group, method: #method, path: #path })
    });
    let version = doc
        .raw
        .pointer("/info/version")
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    Ok(quote! {
        #[allow(clippy::all)]
        pub mod models {
            #models
            #(#aliases)*
        }

        pub trait ApiError: ::serde::Serialize + Send + 'static {
            fn status(&self) -> u16;
        }

        #[derive(Debug, ::serde::Serialize, ::serde::Deserialize)]
        pub enum Never {}
        impl ApiError for Never {
            fn status(&self) -> u16 { match *self {} }
        }

        pub type Result<T, E = Never> = ::core::result::Result<T, E>;

        #(#errors)*

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct Operation {
            pub id: &'static str,
            pub service: &'static str,
            pub method: &'static str,
            pub path: &'static str,
        }
        pub const NETBOX_VERSION: &str = #version;
        pub const OPERATIONS: &[Operation] = &[#(#metadata),*];

        #(#services)*
    })
}
fn implementation(doc: &Doc) -> Result<Tokens, String> {
    let modules = grouped(&doc.ops).into_iter().map(|(group, ops)| {
        let module = format_ident!("{}", snake(&group));
        let server = format_ident!("{}Server", pascal(&group));
        let api = format_ident!("{}", pascal(&group));
        let service_name = format!("netbox.{group}.v1.{}", pascal(&group));
        let methods = ops.iter().map(|op| {
            let method = format_ident!("{}", snake(&op.id));
            let req = format_ident!("{}", op.request);
            let res = format_ident!("{}", op.response);
            let route = format!("/{service_name}/{}", pascal(&op.id));
            quote!(#method(crate::api::models::#req) -> crate::api::models::#res = #route;)
        });
        quote! {
            pub mod #module {
                crate::__netbox_grpc_service! {
                    pub #server {
                        api: crate::api::#api;
                        name: #service_name;
                        #(#methods)*
                    }
                }
                pub fn register<T: crate::api::#api>(
                    app: &mut ::summer::app::AppBuilder,
                    service: T,
                ) {
                    ::summer_grpc::GrpcConfigurator::add_service(app, #server::new(service));
                }
            }
        }
    });
    Ok(quote!(#(#modules)*))
}

fn grouped(ops: &[Op]) -> BTreeMap<String, Vec<&Op>> {
    let mut out: BTreeMap<String, Vec<&Op>> = BTreeMap::new();
    for op in ops {
        out.entry(op.group.clone()).or_default().push(op);
    }
    out
}

fn request_schema(doc: &Value, item: &Value, op: &Value) -> Result<Value, String> {
    let mut parameters: BTreeMap<(String, String), Value> = BTreeMap::new();
    for p in item
        .get("parameters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .chain(
            op.get("parameters")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
        )
    {
        let p = resolve(doc, p)?;
        let name = p
            .get("name")
            .and_then(Value::as_str)
            .ok_or("parameter name missing")?
            .to_owned();
        let place = p
            .get("in")
            .and_then(Value::as_str)
            .unwrap_or("query")
            .to_owned();
        parameters.insert((place, name), p.clone());
    }
    let mut counts = BTreeMap::<String, usize>::new();
    for (_, name) in parameters.keys() {
        *counts.entry(name.clone()).or_default() += 1;
    }

    let mut properties = Map::new();
    let mut required = Vec::new();
    for ((place, name), p) in parameters {
        let field = if counts[&name] > 1 {
            format!("{place}_{name}")
        } else {
            name
        };
        let schema = p.get("schema").cloned().unwrap_or(Value::Bool(true));
        properties.insert(field.clone(), schema);
        if p.get("required").and_then(Value::as_bool).unwrap_or(false) || place == "path" {
            required.push(Value::String(field));
        }
    }

    if let Some(body) = op.get("requestBody") {
        let body = resolve(doc, body)?;
        if let Some(schema) = content_schema(body.get("content")) {
            properties.insert("body".into(), schema);
            if body
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                required.push(Value::String("body".into()));
            }
        }
    }

    Ok(json!({
        "type":"object",
        "properties":properties,
        "required":required,
        "additionalProperties":false
    }))
}

fn response_type(
    doc: &Value,
    op: &Value,
    pool: &mut SchemaPool<'_>,
) -> Result<Model, String> {
    let responses = op
        .get("responses")
        .and_then(Value::as_object)
        .ok_or("operation responses missing")?;
    let mut schemas = Vec::new();
    for (status, response) in responses {
        if !status.starts_with('2') {
            continue;
        }
        let response = resolve(doc, response)?;
        schemas.push(content_schema(response.get("content")));
    }
    if schemas.is_empty() || (schemas.len() == 1 && schemas[0].is_none()) {
        return Ok(Model::Unit);
    }
    if schemas.len() == 1 {
        return pool.intern(schemas.pop().unwrap().unwrap());
    }
    pool.intern(json!({
        "oneOf": schemas
            .into_iter()
            .map(|schema| schema.unwrap_or_else(|| json!({"enum":[null]})))
            .collect::<Vec<_>>()
    }))
}

fn failure_schemas(
    doc: &Value,
    op: &Value,
    pool: &mut SchemaPool<'_>,
) -> Result<Vec<Failure>, String> {
    let responses = op
        .get("responses")
        .and_then(Value::as_object)
        .ok_or("operation responses missing")?;
    let mut failures = Vec::new();
    for (status, response) in responses {
        if status.starts_with('2') {
            continue;
        }
        let Ok(status) = status.parse::<u16>() else {
            continue;
        };
        let response = resolve(doc, response)?;
        let body = match content_schema(response.get("content")) {
            Some(schema) => pool.intern(schema)?,
            None => Model::Unit,
        };
        failures.push(Failure { status, body });
    }
    failures.sort_by_key(|failure| failure.status);
    Ok(failures)
}

fn content_schema(content: Option<&Value>) -> Option<Value> {
    let content = content?.as_object()?;
    content
        .get("application/json")
        .or_else(|| content.values().next())?
        .get("schema")
        .cloned()
}

fn resolve<'a>(doc: &'a Value, value: &'a Value) -> Result<&'a Value, String> {
    let Some(reference) = value.get("$ref").and_then(Value::as_str) else {
        return Ok(value);
    };
    let pointer = reference
        .strip_prefix('#')
        .ok_or_else(|| format!("external ref unsupported: {reference}"))?;
    doc.pointer(pointer)
        .ok_or_else(|| format!("unresolved ref: {reference}"))
}

fn codegen_schema(v: Value) -> Value {
    let Value::Object(mut o) = v else {
        return v;
    };

    // Defaults affect REST behavior, not Rust representation. The exact
    // values remain authoritative in the committed OpenAPI contract.
    o.remove("default");

    // NetBox occasionally emits enum members that cannot satisfy the declared
    // primitive type (notably the string "null" inside integer enums). They
    // are unreachable under OpenAPI's intersecting constraints. Real null
    // members are represented by Progenitor through `nullable`, not as an
    // inner Rust enum variant.
    if let Some(kind) = o.get("type").and_then(Value::as_str).map(str::to_owned)
        && let Some(Value::Array(values)) = o.get_mut("enum")
    {
        values.retain(|value| enum_member_matches(&kind, value));
    }

    if let Some(Value::Object(children)) = o.get_mut("properties") {
        for child in children.values_mut() {
            *child = codegen_schema(child.take());
        }
    }
    for key in ["items", "additionalProperties", "not"] {
        if let Some(child) = o.get_mut(key)
            && child.is_object()
        {
            *child = codegen_schema(child.take());
        }
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(Value::Array(children)) = o.get_mut(key) {
            for child in children {
                *child = codegen_schema(child.take());
            }
        }
    }

    // An empty schema is OpenAPI's unconstrained additional-value schema;
    // Typify turns it into a flattened map instead of dropping extra keys.
    if matches!(o.get("additionalProperties"), Some(Value::Bool(true)))
        && o.get("properties").is_some()
    {
        o.insert("additionalProperties".into(), json!({}));
    }

    Value::Object(o)
}
fn enum_member_matches(kind: &str, value: &Value) -> bool {
    match kind {
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => true,
    }
}

struct SchemaPool<'a> {
    components: &'a mut Map<String, Value>,
    seen: BTreeMap<String, String>,
}

impl<'a> SchemaPool<'a> {
    fn new(components: &'a mut Map<String, Value>) -> Result<Self, String> {
        let mut seen = BTreeMap::new();
        for (name, schema) in components.iter() {
            seen.entry(schema_key(schema)?).or_insert_with(|| name.clone());
        }
        Ok(Self { components, seen })
    }

    fn intern(&mut self, schema: Value) -> Result<Model, String> {
        let schema = codegen_schema(schema);
        if let Some(name) = component_ref(&schema) {
            return Ok(Model::Named(name.to_owned()));
        }

        let key = schema_key(&schema)?;
        if let Some(name) = self.seen.get(&key) {
            return Ok(Model::Named(name.clone()));
        }

        let name = format!("RpcShape{:016x}", fnv1a64(key.as_bytes()));
        if let Some(existing) = self.components.get(&name)
            && existing != &schema
        {
            return Err(format!("schema hash collision for {name}"));
        }
        self.components.insert(name.clone(), schema);
        self.seen.insert(key, name.clone());
        Ok(Model::Named(name))
    }
}

fn schema_key(schema: &Value) -> Result<String, String> {
    serde_json::to_string(schema).map_err(|error| error.to_string())
}

fn component_ref(schema: &Value) -> Option<&str> {
    schema
        .get("$ref")
        .and_then(Value::as_str)?
        .strip_prefix("#/components/schemas/")
}

fn model_alias(alias: &str, target: &Model) -> Tokens {
    let alias = format_ident!("{}", alias);
    match target {
        Model::Named(target) => {
            let target = format_ident!("{}", target);
            quote!(pub type #alias = #target;)
        }
        Model::Unit => quote!(pub type #alias = ();),
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn group(path: &str) -> String {
    path.trim_matches('/')
        .split('/')
        .find(|s| *s != "api")
        .filter(|s| !s.is_empty())
        .unwrap_or("root")
        .replace('-', "_")
}

fn fallback_id(method: &str, path: &str) -> String {
    format!(
        "{}_{}",
        method,
        path.trim_matches('/')
            .chars()
            .map(|c| if matches!(c, '/' | '{' | '}' | '-') {
                '_'
            } else {
                c
            })
            .collect::<String>()
    )
}

fn pascal(s: &str) -> String {
    s.to_pascal_case()
}
fn snake(s: &str) -> String {
    s.to_snake_case()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delegates_openapi_semantics_to_progenitor() {
        assert_eq!(
            codegen_schema(json!({"type":"string","nullable":true})),
            json!({"type":"string","nullable":true})
        );
        assert_eq!(
            codegen_schema(json!({"$ref":"#/components/schemas/Device"})),
            json!({"$ref":"#/components/schemas/Device"})
        );
        assert_eq!(
            codegen_schema(json!({
                "type":"integer", "minimum":1, "exclusiveMinimum":true
            })),
            json!({
                "type":"integer", "minimum":1, "exclusiveMinimum":true
            })
        );
    }

    #[test]
    fn strips_codegen_default_without_touching_property_named_default() {
        let normalized = codegen_schema(json!({
            "type":"object",
            "properties":{"default":{"type":"string","default":"inner"}}
        }));
        assert_eq!(
            normalized["properties"]["default"],
            json!({"type":"string"})
        );
    }

    #[test]
    fn removes_unreachable_null_enum_member() {
        assert_eq!(
            codegen_schema(json!({
                "type":"integer",
                "enum":[1200, 2400, null]
            })),
            json!({
                "type":"integer",
                "enum":[1200, 2400]
            })
        );
        assert_eq!(
            codegen_schema(json!({
                "type":"integer",
                "nullable":true,
                "enum":[1200, null]
            }))["enum"],
            json!([1200])
        );
        assert_eq!(
            codegen_schema(json!({
                "type":"integer",
                "nullable":true,
                "enum":[1200, "null", null]
            }))["enum"],
            json!([1200])
        );
        assert_eq!(
            codegen_schema(json!({
                "type":"string",
                "nullable":true,
                "enum":["A", "null", null]
            }))["enum"],
            json!(["A", "null"])
        );
    }

    #[test]
    fn preserves_open_object_with_named_fields() {
        assert_eq!(
            codegen_schema(json!({
                "type":"object",
                "properties":{"id":{"type":"integer"}},
                "additionalProperties":true
            }))["additionalProperties"],
            json!({})
        );
    }

    #[test]
    fn naming_is_stable() {
        assert_eq!(pascal("dcim_devices_retrieve"), "DcimDevicesRetrieve");
        assert_eq!(snake("dcim_devices_retrieve"), "dcim_devices_retrieve");
        assert_eq!(group("/api/dcim/devices/{id}/"), "dcim");
    }
}
