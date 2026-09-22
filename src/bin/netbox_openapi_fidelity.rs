use serde_json::Value;
use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const OPTIONS_FILE: &str = "netbox_openapi_options.proto";
const DOCUMENT_FILE: &str = "netbox_openapi_document.proto";
const MANIFEST_FILE: &str = ".netbox-openapi-proto-manifest";
const SOURCE_CHUNK_LEN: usize = 16 * 1024;

#[derive(Debug)]
struct Config {
    input: String,
    output_dir: PathBuf,
    package: String,
    check: bool,
    verify_generated: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("netbox_openapi_fidelity: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let config = parse_args(env::args().skip(1))?;
    let source = read_input(&config.input)?;
    let document: Value = serde_json::from_str(&source)?;
    validate_openapi(&document)?;

    let files = generate_fidelity_files(&source, &document, &config.package)?;

    if config.check {
        check_files(&files, &config.output_dir)?;
    } else {
        write_files(&files, &config.output_dir)?;
    }

    if config.verify_generated || config.check {
        verify_generated_document(&config.output_dir.join(DOCUMENT_FILE), source.as_bytes())?;
    }

    Ok(())
}

fn usage() -> &'static str {
    "Usage: netbox_openapi_fidelity --input <schema.json|-> --output-dir <dir> [--package netbox.v1] [--check] [--verify-generated]"
}

fn parse_args<I>(args: I) -> Result<Config, Box<dyn Error>>
where
    I: IntoIterator<Item = String>,
{
    let mut input = None;
    let mut output_dir = None;
    let mut package = "netbox.v1".to_string();
    let mut check = false;
    let mut verify_generated = false;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = Some(next_arg(&mut args, "--input")?),
            "--output-dir" => {
                output_dir = Some(PathBuf::from(next_arg(&mut args, "--output-dir")?))
            }
            "--package" => package = next_arg(&mut args, "--package")?,
            "--check" => check = true,
            "--verify-generated" => verify_generated = true,
            "--help" | "-h" => {
                println!("{}", usage());
                std::process::exit(0);
            }
            unknown => return Err(format!("unknown argument {unknown:?}\n{}", usage()).into()),
        }
    }

    validate_package(&package)?;

    Ok(Config {
        input: input.ok_or_else(|| format!("--input is required\n{}", usage()))?,
        output_dir: output_dir.ok_or_else(|| format!("--output-dir is required\n{}", usage()))?,
        package,
        check,
        verify_generated,
    })
}

fn next_arg<I>(args: &mut I, flag: &str) -> Result<String, Box<dyn Error>>
where
    I: Iterator<Item = String>,
{
    args.next()
        .ok_or_else(|| format!("{flag} requires a value").into())
}

fn validate_package(package: &str) -> Result<(), Box<dyn Error>> {
    let valid = !package.is_empty()
        && package.split('.').all(|part| {
            !part.is_empty()
                && !part
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_digit())
                && part
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        });

    if valid {
        Ok(())
    } else {
        Err(format!("invalid protobuf package {package:?}").into())
    }
}

fn read_input(input: &str) -> Result<String, Box<dyn Error>> {
    if input == "-" {
        let mut value = String::new();
        io::stdin().read_to_string(&mut value)?;
        return Ok(value);
    }

    Ok(fs::read_to_string(input)?)
}

fn validate_openapi(document: &Value) -> Result<(), Box<dyn Error>> {
    let version = document
        .get("openapi")
        .and_then(Value::as_str)
        .ok_or("input is missing the OpenAPI version")?;

    if !version.starts_with("3.") {
        return Err(format!("only OpenAPI 3.x is supported, got {version}").into());
    }

    if document.get("paths").and_then(Value::as_object).is_none() {
        return Err("input is missing an OpenAPI paths object".into());
    }

    Ok(())
}

fn generate_fidelity_files(
    source: &str,
    document: &Value,
    package: &str,
) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let encoded = base64_encode(source.as_bytes());
    let decoded = base64_decode(&encoded)?;
    if decoded != source.as_bytes() {
        return Err("internal fidelity failure: base64 round trip changed the OpenAPI source".into());
    }

    let mut files = Vec::new();
    files.push((
        OPTIONS_FILE.to_string(),
        generate_options_proto(document, package),
    ));
    files.push((
        DOCUMENT_FILE.to_string(),
        generate_document_proto(source, document, package, &encoded),
    ));
    Ok(files)
}

fn generate_options_proto(document: &Value, package: &str) -> String {
    let mut out = String::new();
    write_header(&mut out, document, package);
    out.push_str("import \"google/protobuf/descriptor.proto\";\n\n");
    out.push_str(
        "message OpenApiDocument {\n\
         \x20 string media_type = 1;\n\
         \x20 string encoding = 2;\n\
         \x20 uint64 byte_length = 3;\n\
         \x20 string fnv1a64_hex = 4;\n\
         \x20 repeated string chunks = 5;\n\
         }\n\n",
    );
    out.push_str(
        "extend google.protobuf.FileOptions {\n\
         \x20 OpenApiDocument openapi_document = 51000;\n\
         \x20 string openapi_file_role = 51001;\n\
         }\n",
    );
    out
}

fn generate_document_proto(
    source: &str,
    document: &Value,
    package: &str,
    encoded: &str,
) -> String {
    let mut out = String::new();
    write_header(&mut out, document, package);
    out.push_str(&format!("import \"{OPTIONS_FILE}\";\n\n"));
    out.push_str(&format!(
        "option ({package}.openapi_file_role) = \"lossless-source\";\n"
    ));
    out.push_str(&format!("option ({package}.openapi_document) = {{\n"));
    out.push_str("  media_type: \"application/vnd.oai.openapi+json\"\n");
    out.push_str("  encoding: \"base64\"\n");
    out.push_str(&format!("  byte_length: {}\n", source.len()));
    out.push_str(&format!(
        "  fnv1a64_hex: \"{:016x}\"\n",
        fnv1a64(source.as_bytes())
    ));
    for chunk in ascii_chunks(encoded, SOURCE_CHUNK_LEN) {
        out.push_str("  chunks: \"");
        out.push_str(chunk);
        out.push_str("\"\n");
    }
    out.push_str("};\n");
    out
}

fn write_header(out: &mut String, document: &Value, package: &str) {
    let version = document
        .pointer("/info/version")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    out.push_str("// @generated by netbox_openapi_fidelity; DO NOT EDIT.\n");
    out.push_str(&format!("// NetBox OpenAPI version: {version}\n"));
    out.push_str("// This file is part of the byte-for-byte lossless OpenAPI preservation layer.\n\n");
    out.push_str("syntax = \"proto3\";\n\n");
    out.push_str(&format!("package {package};\n\n"));
}

fn write_files(files: &[(String, String)], output_dir: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(output_dir)?;

    for (name, content) in files {
        let path = output_dir.join(name);
        if fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
            fs::write(path, content)?;
        }
    }

    let manifest_path = output_dir.join(MANIFEST_FILE);
    let mut names = read_manifest_names(&manifest_path);
    for (name, _) in files {
        names.insert(name.clone());
    }
    let manifest = names.into_iter().collect::<Vec<_>>().join("\n") + "\n";
    fs::write(manifest_path, manifest)?;

    Ok(())
}

fn check_files(files: &[(String, String)], output_dir: &Path) -> Result<(), Box<dyn Error>> {
    let mut mismatches = Vec::new();

    for (name, expected) in files {
        let path = output_dir.join(name);
        match fs::read_to_string(&path) {
            Ok(actual) if actual == *expected => {}
            Ok(_) => mismatches.push(format!("changed: {}", path.display())),
            Err(_) => mismatches.push(format!("missing: {}", path.display())),
        }
    }

    let manifest_path = output_dir.join(MANIFEST_FILE);
    let names = read_manifest_names(&manifest_path);
    for (name, _) in files {
        if !names.contains(name) {
            mismatches.push(format!(
                "manifest missing generated fidelity file {name}: {}",
                manifest_path.display()
            ));
        }
    }

    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "lossless OpenAPI protobuf sidecars are stale:\n{}",
            mismatches
                .into_iter()
                .map(|value| format!("  {value}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
        .into())
    }
}

fn read_manifest_names(path: &Path) -> BTreeSet<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn verify_generated_document(path: &Path, expected: &[u8]) -> Result<(), Box<dyn Error>> {
    let proto = fs::read_to_string(path)?;
    let mut encoded = String::new();
    let mut byte_length = None;
    let mut hash = None;

    for line in proto.lines().map(str::trim) {
        if let Some(value) = line.strip_prefix("byte_length: ") {
            byte_length = Some(value.parse::<usize>()?);
        } else if let Some(value) = line
            .strip_prefix("fnv1a64_hex: \"")
            .and_then(|value| value.strip_suffix('"'))
        {
            hash = Some(value.to_string());
        } else if let Some(value) = line
            .strip_prefix("chunks: \"")
            .and_then(|value| value.strip_suffix('"'))
        {
            encoded.push_str(value);
        }
    }

    if byte_length != Some(expected.len()) {
        return Err(format!(
            "generated OpenAPI byte length mismatch: expected {}, got {:?}",
            expected.len(),
            byte_length
        )
        .into());
    }

    let expected_hash = format!("{:016x}", fnv1a64(expected));
    if hash.as_deref() != Some(expected_hash.as_str()) {
        return Err(format!(
            "generated OpenAPI hash mismatch: expected {expected_hash}, got {hash:?}"
        )
        .into());
    }

    let decoded = base64_decode(&encoded)?;
    if decoded != expected {
        return Err("generated protobuf does not reconstruct the OpenAPI input byte-for-byte".into());
    }

    Ok(())
}

fn ascii_chunks(value: &str, chunk_len: usize) -> impl Iterator<Item = &str> {
    (0..value.len())
        .step_by(chunk_len)
        .map(move |start| &value[start..usize::min(start + chunk_len, value.len())])
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut chunks = bytes.chunks_exact(3);

    for chunk in &mut chunks {
        let value =
            (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        out.push(TABLE[((value >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((value >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((value >> 6) & 0x3f) as usize] as char);
        out.push(TABLE[(value & 0x3f) as usize] as char);
    }

    match chunks.remainder() {
        [] => {}
        [first] => {
            let value = u32::from(*first) << 16;
            out.push(TABLE[((value >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((value >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        [first, second] => {
            let value = (u32::from(*first) << 16) | (u32::from(*second) << 8);
            out.push(TABLE[((value >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((value >> 12) & 0x3f) as usize] as char);
            out.push(TABLE[((value >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => unreachable!(),
    }

    out
}

fn base64_decode(value: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    if !value.len().is_multiple_of(4) {
        return Err("invalid base64 length".into());
    }

    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(value.len() / 4 * 3);

    for chunk in bytes.chunks_exact(4) {
        let a = decode_base64_character(chunk[0])?;
        let b = decode_base64_character(chunk[1])?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            decode_base64_character(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            decode_base64_character(chunk[3])?
        };

        let packed = (u32::from(a) << 18)
            | (u32::from(b) << 12)
            | (u32::from(c) << 6)
            | u32::from(d);

        out.push(((packed >> 16) & 0xff) as u8);
        if chunk[2] != b'=' {
            out.push(((packed >> 8) & 0xff) as u8);
        }
        if chunk[3] != b'=' {
            out.push((packed & 0xff) as u8);
        }

        if chunk[2] == b'=' && chunk[3] != b'=' {
            return Err("invalid base64 padding".into());
        }
    }

    Ok(out)
}

fn decode_base64_character(value: u8) -> Result<u8, Box<dyn Error>> {
    match value {
        b'A'..=b'Z' => Ok(value - b'A'),
        b'a'..=b'z' => Ok(value - b'a' + 26),
        b'0'..=b'9' => Ok(value - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(format!("invalid base64 character 0x{value:02x}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_document() -> Value {
        serde_json::json!({
            "openapi": "3.0.3",
            "info": {
                "title": "NetBox",
                "version": "4.7-test"
            },
            "paths": {
                "/api/dcim/devices/": {
                    "get": {
                        "operationId": "dcim_devices_list",
                        "security": [{"tokenAuth": []}],
                        "responses": {
                            "200": {
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "oneOf": [
                                                {"type": "string"},
                                                {"type": "null"}
                                            ]
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn base64_round_trip_is_byte_exact() {
        for value in [
            "",
            "f",
            "fo",
            "foo",
            "NetBox\n",
            "{\"unicode\":\"λ\",\"escaped\":\"\\\\n\"}\r\n",
        ] {
            let encoded = base64_encode(value.as_bytes());
            let decoded = base64_decode(&encoded).unwrap();
            assert_eq!(decoded, value.as_bytes());
        }
    }

    #[test]
    fn generated_document_contains_full_source() {
        let document = sample_document();
        let source = serde_json::to_string_pretty(&document).unwrap();
        let files = generate_fidelity_files(&source, &document, "netbox.v1").unwrap();
        let generated = files
            .iter()
            .find(|(name, _)| name == DOCUMENT_FILE)
            .map(|(_, content)| content)
            .unwrap();

        let encoded = generated
            .lines()
            .map(str::trim)
            .filter_map(|line| {
                line.strip_prefix("chunks: \"")
                    .and_then(|value| value.strip_suffix('"'))
            })
            .collect::<String>();

        assert_eq!(base64_decode(&encoded).unwrap(), source.as_bytes());
        assert!(generated.contains(&format!("byte_length: {}", source.len())));
        assert!(generated.contains(&format!(
            "fnv1a64_hex: \"{:016x}\"",
            fnv1a64(source.as_bytes())
        )));
    }

    #[test]
    fn package_validation_rejects_invalid_identifiers() {
        for invalid in ["", "netbox..v1", "1netbox.v1", "netbox.v-1", ".netbox"] {
            assert!(validate_package(invalid).is_err(), "{invalid}");
        }
        for valid in ["netbox.v1", "netbox_v1", "company.netbox.v2"] {
            assert!(validate_package(valid).is_ok(), "{valid}");
        }
    }

    #[test]
    fn chunks_are_reassemblable_without_escape_processing() {
        let value = "A".repeat(SOURCE_CHUNK_LEN * 2 + 17);
        let rebuilt = ascii_chunks(&value, SOURCE_CHUNK_LEN).collect::<String>();
        assert_eq!(rebuilt, value);
    }
}
