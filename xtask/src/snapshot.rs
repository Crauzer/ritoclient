//! `cargo xtask ritoclient-snapshot` - capture the generator's input from a
//! live client into `schema/`.
//!
//! The snapshot is checked in, filtered to the in-scope namespaces, so that
//! codegen is offline and version drift shows up as a reviewable diff. See
//! `docs/plans/api-surface-codegen.md`, step 2.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use ritoclient_core::{Client, Presence, live_lockfile};
use serde_json::{Map, Value, json};

use crate::surface;

pub fn run(args: &[String]) -> Result<(), String> {
    let mut client_build = String::from("unknown");
    let mut schema_dir = PathBuf::from("schema");

    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--client-build" => {
                client_build = args.next().ok_or("--client-build needs a value")?.clone();
            }
            "--schema-dir" => {
                schema_dir = PathBuf::from(args.next().ok_or("--schema-dir needs a value")?);
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }

    let lockfile = live_lockfile().ok_or(
        "no live Riot Client - the snapshot reads the client's own self-description, \
         so one has to be running",
    )?;
    println!(
        "Riot Client pid {} on port {} ({})",
        lockfile.pid, lockfile.port, lockfile.protocol
    );

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    ensure_awake(&client)?;

    println!("fetching /help?format=Full ...");
    let help = fetch_json(&client, "/help?format=Full")?;
    println!("fetching /swagger/v3/openapi.json ...");
    let openapi = fetch_json(&client, "/swagger/v3/openapi.json")?;

    let region = client
        .get_json::<Value>("/riotclient/region-locale")
        .unwrap_or(Value::Null);

    let kebabs: Vec<String> = surface::IN_SCOPE
        .iter()
        .map(|ns| surface::kebab(ns))
        .collect();

    let (help_filtered, function_count, type_count) = filter_help(&help, &kebabs)?;
    let openapi_filtered = filter_openapi(&openapi, &kebabs)?;

    let date = today();
    println!("probing derived paths ...");
    let probes = probe_paths(&client, &help_filtered, &date, &client_build);

    std::fs::create_dir_all(&schema_dir)
        .map_err(|e| format!("could not create {}: {e}", schema_dir.display()))?;

    write_json(&schema_dir.join("help.filtered.json"), &help_filtered)?;
    write_json(&schema_dir.join("openapi.filtered.json"), &openapi_filtered)?;
    write_json(&schema_dir.join("probes.json"), &probes)?;

    let overrides = schema_dir.join("overrides.toml");
    if !overrides.exists() {
        write_text(&overrides, OVERRIDES_SKELETON)?;
    }

    let snapshot_md = render_snapshot_md(
        &date,
        &client_build,
        &region,
        function_count,
        type_count,
        &help,
        &openapi_filtered,
        &probes,
    );
    write_text(&schema_dir.join("SNAPSHOT.md"), &snapshot_md)?;

    println!("snapshot written to {}", schema_dir.display());
    Ok(())
}

/// A tray-idle client's surface collapses to the argv handoff plus the
/// remoting builtins - nothing worth snapshotting. Waking it is the documented
/// duplicate-instance path: `new-args` with an argv that launches nothing.
/// The wake restarts the remoting listener on a new port; the core client
/// re-reads the lockfile per attempt, which is what makes the polling work.
fn ensure_awake(client: &Client) -> Result<(), String> {
    if function_count(client) >= FULL_SURFACE_FLOOR {
        return Ok(());
    }

    println!("the client is tray-idle; waking it (this opens its window) ...");
    let response = client
        .post("/riotclientapp/v1/new-args")
        .json::<[&str]>(&[])
        .send()
        .map_err(|e| format!("could not wake the client: {e}"))?;
    if !response.is_success() {
        return Err(format!(
            "the wake was refused: HTTP {} {}",
            response.status(),
            response.body().trim()
        ));
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    while std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(1));
        let count = function_count(client);
        if count >= FULL_SURFACE_FLOOR {
            println!("awake: {count} functions registered");
            // Swagger and the last plugins register a beat after /help fills
            // out; give them a moment rather than snapshotting a half-booted
            // surface.
            std::thread::sleep(Duration::from_secs(5));
            return Ok(());
        }
    }
    Err("woke the client but its API surface never filled out (90s)".to_string())
}

/// Well below the 1261 of a booted client, well above the 8 of a tray-idle one.
const FULL_SURFACE_FLOOR: usize = 500;

fn function_count(client: &Client) -> usize {
    let Some(help) = client.get_json::<Value>("/help") else {
        return 0;
    };
    let help = undouble(help);
    match help.get("functions") {
        Some(Value::Array(functions)) => functions.len(),
        Some(Value::Object(functions)) => functions.len(),
        _ => 0,
    }
}

fn fetch_json(client: &Client, path: &str) -> Result<Value, String> {
    let response = client
        .get(path)
        .send()
        .map_err(|e| format!("GET {path}: {e}"))?;
    if !response.is_success() {
        return Err(format!(
            "GET {path} answered HTTP {}: {}",
            response.status(),
            response.body().trim()
        ));
    }
    let value: Value = serde_json::from_str(response.body())
        .map_err(|e| format!("GET {path}: body is not JSON: {e}"))?;
    Ok(undouble(value))
}

/// Both `/help` and swagger bodies have been observed double-encoded - a JSON
/// string whose contents are the document - and single-encoded, depending on
/// build. Try the second parse, keep what we have on failure.
fn undouble(value: Value) -> Value {
    match value {
        Value::String(inner) => serde_json::from_str(&inner).unwrap_or(Value::String(inner)),
        other => other,
    }
}

/// Keep the in-scope functions, the transitive closure of the types they
/// reference, and the in-scope namespaces' events. Sorted by name so a re-take
/// diffs cleanly.
fn filter_help(help: &Value, kebabs: &[String]) -> Result<(Value, usize, usize), String> {
    let functions = as_array(help, "functions")?;
    let types = as_array(help, "types")?;
    let events = as_array(help, "events")?;

    let mut kept_functions: Vec<Value> = functions
        .iter()
        .filter(|f| {
            name_of(f)
                .and_then(surface::parse_function)
                .is_some_and(|parsed| surface::IN_SCOPE.contains(&parsed.namespace.as_str()))
        })
        .cloned()
        .collect();
    kept_functions.sort_by_key(|f| name_of(f).unwrap_or_default().to_string());

    let type_index: BTreeMap<&str, &Value> = types
        .iter()
        .filter_map(|t| name_of(t).map(|n| (n, t)))
        .collect();

    let mut closure: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<String> = Vec::new();
    for function in &kept_functions {
        collect_function_refs(function, &mut queue);
    }
    while let Some(name) = queue.pop() {
        if !type_index.contains_key(name.as_str()) || !closure.insert(name.clone()) {
            continue;
        }
        if let Some(fields) = type_index[name.as_str()]
            .get("fields")
            .and_then(Value::as_array)
        {
            for field in fields {
                collect_type_ref(field.get("type"), &mut queue);
            }
        }
    }

    let kept_types: Vec<Value> = closure
        .iter()
        .map(|name| (*type_index[name.as_str()]).clone())
        .collect();

    let mut kept_events: Vec<Value> = events
        .iter()
        .filter(|e| {
            name_of(e)
                .and_then(|n| n.strip_prefix("OnJsonApiEvent_"))
                .is_some_and(|rest| {
                    kebabs.iter().any(|k| {
                        rest.strip_prefix(k.as_str())
                            .is_some_and(|r| r.starts_with('_'))
                    })
                })
        })
        .cloned()
        .collect();
    kept_events.sort_by_key(|e| name_of(e).unwrap_or_default().to_string());

    let function_count = kept_functions.len();
    let type_count = kept_types.len();
    let filtered = json!({
        "events": kept_events,
        "functions": kept_functions,
        "types": kept_types,
    });
    Ok((filtered, function_count, type_count))
}

fn as_array<'a>(doc: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    doc.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("the help document has no `{key}` array"))
}

fn name_of(entry: &Value) -> Option<&str> {
    entry.get("name").and_then(Value::as_str)
}

fn collect_function_refs(function: &Value, out: &mut Vec<String>) {
    if let Some(arguments) = function.get("arguments").and_then(Value::as_array) {
        for argument in arguments {
            collect_type_ref(argument.get("type"), out);
        }
    }
    collect_type_ref(function.get("returns"), out);
}

fn collect_type_ref(type_desc: Option<&Value>, out: &mut Vec<String>) {
    let Some(type_desc) = type_desc else { return };
    for key in ["type", "elementType"] {
        // Not a let-chain: the workspace MSRV (1.85) predates their
        // stabilization, and CI checks the whole workspace at MSRV.
        if let Some(name) = type_desc.get(key).and_then(Value::as_str) {
            if !name.is_empty() {
                out.push(name.to_string());
            }
        }
    }
}

/// Keep the in-scope paths and the `$ref` closure of the schemas they use.
/// Swagger covers only a few of the eleven namespaces; that is expected and is
/// why `/help` is the authoritative index.
fn filter_openapi(openapi: &Value, kebabs: &[String]) -> Result<Value, String> {
    let paths = openapi
        .get("paths")
        .and_then(Value::as_object)
        .ok_or("the openapi document has no `paths` object")?;

    let kept_paths: Map<String, Value> = paths
        .iter()
        .filter(|(path, _)| {
            path.split('/')
                .nth(1)
                .is_some_and(|first| kebabs.iter().any(|k| k == first))
        })
        .map(|(path, item)| (path.clone(), item.clone()))
        .collect();

    let schemas = openapi
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut closure: BTreeSet<String> = BTreeSet::new();
    let mut queue: Vec<String> = Vec::new();
    for item in kept_paths.values() {
        collect_schema_refs(item, &mut queue);
    }
    while let Some(name) = queue.pop() {
        if !schemas.contains_key(&name) || !closure.insert(name.clone()) {
            continue;
        }
        collect_schema_refs(&schemas[&name], &mut queue);
    }

    let kept_schemas: Map<String, Value> = closure
        .iter()
        .map(|name| (name.clone(), schemas[name].clone()))
        .collect();

    Ok(json!({
        "info": openapi.get("info").cloned().unwrap_or(Value::Null),
        "openapi": openapi.get("openapi").cloned().unwrap_or(Value::Null),
        "paths": kept_paths,
        "components": { "schemas": kept_schemas },
    }))
}

fn collect_schema_refs(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                if key == "$ref" {
                    if let Some(name) = inner
                        .as_str()
                        .and_then(|r| r.strip_prefix("#/components/schemas/"))
                    {
                        out.push(name.to_string());
                    }
                }
                collect_schema_refs(inner, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_schema_refs(item, out);
            }
        }
        _ => {}
    }
}

/// GET every parameterless derived path and record what the client said.
///
/// A plain GET never invokes a mutating handler - a POST-only route answers
/// 405, which still proves the spelling. Parameterized paths are skipped: a
/// literal `{placeholder}` answers as no such route, so probing one says
/// nothing about the spelling.
fn probe_paths(client: &Client, help_filtered: &Value, date: &str, client_build: &str) -> Value {
    let functions = help_filtered
        .get("functions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut entries: Vec<Value> = Vec::new();
    for function in &functions {
        let Some(name) = name_of(function) else {
            continue;
        };
        let Some(parsed) = surface::parse_function(name) else {
            continue;
        };
        let arg_names: Vec<&str> = function
            .get("arguments")
            .and_then(Value::as_array)
            .map(|args| {
                args.iter()
                    .filter_map(|a| a.get("name").and_then(Value::as_str))
                    .collect()
            })
            .unwrap_or_default();

        let segments = surface::derive_segments(&parsed.rest, &arg_names);
        let resource = surface::resource(&segments);
        let path = if resource.is_empty() {
            format!("/{}/v{}", surface::kebab(&parsed.namespace), parsed.version)
        } else {
            format!(
                "/{}/v{}/{resource}",
                surface::kebab(&parsed.namespace),
                parsed.version
            )
        };

        if resource.contains('{') {
            entries.push(json!({
                "function": name,
                "method": parsed.verb.to_uppercase(),
                "path": path,
                "probe": "skipped: parameterized path",
            }));
            continue;
        }

        let entry = match client.get(&path).timeout(Duration::from_secs(5)).send() {
            Ok(response) => {
                let presence = match Presence::from(&response) {
                    Presence::Serving => "serving",
                    Presence::Registered => "registered",
                    Presence::Absent => "absent",
                };
                let error_code = response
                    .riot_error()
                    .map(|e| Value::String(e.error_code))
                    .unwrap_or(Value::Null);
                json!({
                    "function": name,
                    "method": parsed.verb.to_uppercase(),
                    "path": path,
                    "probedWith": "GET",
                    "status": response.status().as_u16(),
                    "errorCode": error_code,
                    "presence": presence,
                    "date": date,
                    "clientBuild": client_build,
                })
            }
            Err(e) => json!({
                "function": name,
                "method": parsed.verb.to_uppercase(),
                "path": path,
                "probe": format!("failed: {e}"),
            }),
        };
        entries.push(entry);
    }

    entries.sort_by_key(|e| {
        e.get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    });
    json!({ "probes": entries })
}

#[allow(clippy::too_many_arguments)]
fn render_snapshot_md(
    date: &str,
    client_build: &str,
    region: &Value,
    function_count: usize,
    type_count: usize,
    help_full: &Value,
    openapi_filtered: &Value,
    probes: &Value,
) -> String {
    let total_functions = help_full
        .get("functions")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let total_types = help_full
        .get("types")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let openapi_paths = openapi_filtered
        .get("paths")
        .and_then(Value::as_object)
        .map_or(0, Map::len);
    let probed = probes
        .get("probes")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let region_text = region
        .get("region")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let locale_text = region
        .get("locale")
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    format!(
        "# Schema snapshot\n\
         \n\
         Taken with `cargo xtask ritoclient-snapshot` against a live client. Do not edit the\n\
         `.json` files by hand - re-take the snapshot instead and read the diff; that diff is\n\
         the version-drift check. `overrides.toml` is the opposite: hand-authored, never\n\
         overwritten by the snapshot.\n\
         \n\
         | Provenance | |\n\
         | --- | --- |\n\
         | Date | {date} |\n\
         | Riot Client build | {client_build} |\n\
         | Region / locale | {region_text} / {locale_text} |\n\
         \n\
         | Contents | |\n\
         | --- | --- |\n\
         | `help.filtered.json` | {function_count} of {total_functions} functions, \
         {type_count} of {total_types} types (in-scope namespaces + transitive type closure) |\n\
         | `openapi.filtered.json` | {openapi_paths} paths - swagger covers only a few in-scope \
         namespaces, which is why `/help` is the index |\n\
         | `probes.json` | {probed} derived paths, parameterless ones GET-probed |\n\
         \n\
         The surface depends on client state: a tray-idle client serves almost nothing, so the\n\
         snapshot wakes it first, and entitlements can hide routes per account and region -\n\
         which is why the region is provenance and not trivia.\n"
    )
}

const OVERRIDES_SKELETON: &str = "\
# Hand-authored corrections and measured knowledge for `cargo xtask
# ritoclient-codegen`. This file is data - names, spellings, prose - never
# code, and the snapshot never overwrites it.

# [[namespace]]
# id = \"product-launcher\"        # the client's kebab spelling
# module = \"product_launcher\"    # the Rust module name
# ...
";

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|e| format!("could not serialize {}: {e}", path.display()))?;
    text.push('\n');
    write_text(path, &text)
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// Today's civil date, UTC, without a chrono dependency (Howard Hinnant's
/// days-from-epoch algorithm).
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undouble_unwraps_a_double_encoded_body() {
        let doubled = Value::String("{\"a\": 1}".to_string());
        assert_eq!(undouble(doubled), json!({"a": 1}));

        // A plain string body stays a string rather than erroring.
        let plain = Value::String("C:/logs/path".to_string());
        assert_eq!(undouble(plain), Value::String("C:/logs/path".to_string()));
    }

    #[test]
    fn the_date_is_civil() {
        // 2026-07-29 00:00:00 UTC.
        let days_to_2026_07_29 = 20_663;
        let z = days_to_2026_07_29 + 719_468_i64;
        assert_eq!(z.div_euclid(146_097), 5);
        assert!(today().starts_with("20"));
    }
}
