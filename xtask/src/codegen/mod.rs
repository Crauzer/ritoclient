//! `cargo xtask ritoclient-codegen` - regenerate `crates/ritoclient-api/src`
//! from `schema/`. Offline.
//!
//! Everything under `src/` is wiped and rewritten; `Cargo.toml` is the crate's
//! only hand-written file. The measured doc prose comes from
//! `schema/overrides.toml` - the templates here decide *where* it goes, never
//! what it says. The last step runs `cargo fmt` on the crate, so byte fidelity
//! against the hand-written fixture comes from the same formatter CI enforces
//! rather than from these emitters imitating it.

mod templates;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::surface::{self, Segment};

pub fn run(args: &[String]) -> Result<(), String> {
    let mut schema_dir = PathBuf::from("schema");
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--schema-dir" => {
                schema_dir = PathBuf::from(args.next().ok_or("--schema-dir needs a value")?);
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }

    let schema = Schema::load(&schema_dir.join("help.filtered.json"))?;
    let overrides = Overrides::load(&schema_dir.join("overrides.toml"))?;

    let resolved = resolve(&schema, &overrides)?;
    let files = emit(&resolved);

    let src = Path::new("crates/ritoclient-api/src");
    if src.exists() {
        std::fs::remove_dir_all(src)
            .map_err(|e| format!("could not wipe {}: {e}", src.display()))?;
    }
    for (path, text) in &files {
        let path = src.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
        }
        std::fs::write(&path, text)
            .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    }
    println!("wrote {} files under {}", files.len(), src.display());

    let status = std::process::Command::new("cargo")
        .args(["fmt", "--package", "ritoclient-api"])
        .status()
        .map_err(|e| format!("could not run cargo fmt: {e}"))?;
    if !status.success() {
        return Err("cargo fmt failed on the generated crate".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Inputs

/// The filtered `/help?format=Full` snapshot.
struct Schema {
    functions: BTreeMap<String, Function>,
    types: BTreeMap<String, TypeDef>,
}

#[derive(Deserialize)]
struct Function {
    name: String,
    arguments: Vec<Argument>,
    returns: TypeRef,
}

#[derive(Deserialize)]
struct Argument {
    name: String,
    #[serde(rename = "type")]
    ty: TypeRef,
}

#[derive(Deserialize, Clone)]
struct TypeRef {
    #[serde(rename = "type")]
    ty: String,
    #[serde(rename = "elementType", default)]
    element: String,
}

#[derive(Deserialize)]
struct TypeDef {
    name: String,
    #[serde(default)]
    fields: Vec<FieldDef>,
    #[serde(default)]
    values: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct FieldDef {
    name: String,
    #[serde(rename = "type")]
    ty: TypeRef,
}

impl Schema {
    fn load(path: &Path) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Doc {
            functions: Vec<Function>,
            types: Vec<TypeDef>,
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let doc: Doc = serde_json::from_str(&text)
            .map_err(|e| format!("{} does not parse: {e}", path.display()))?;
        Ok(Self {
            functions: doc
                .functions
                .into_iter()
                .map(|f| (f.name.clone(), f))
                .collect(),
            types: doc.types.into_iter().map(|t| (t.name.clone(), t)).collect(),
        })
    }
}

/// `schema/overrides.toml`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Overrides {
    namespace: Vec<NsOverride>,
    #[serde(rename = "type", default)]
    types: Vec<TypeOverride>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NsOverride {
    id: String,
    module: String,
    accessor_doc: String,
    module_doc: String,
    #[serde(default)]
    models_doc: Option<String>,
    #[serde(default)]
    timeout_const: Option<TimeoutConst>,
    endpoint: Vec<EndpointOverride>,
    #[serde(default)]
    tests: Option<BindingTests>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeoutConst {
    name: String,
    secs: u64,
    doc: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EndpointOverride {
    #[serde(rename = "fn")]
    function: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    method_name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    timeout: Option<String>,
    /// A confirmed resource spelling, for the paths the name-derivation gets
    /// wrong (`quit/switch-background-mode`). Swagger's spelling should win
    /// where swagger has one; record it here after checking `probes.json`.
    #[serde(default)]
    resource: Option<String>,
    route_doc: String,
    endpoint_doc: String,
    method_doc: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingTests {
    name: String,
    case: Vec<BindingCase>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingCase {
    var: String,
    endpoint: String,
    args: Vec<String>,
    expect: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TypeOverride {
    name: String,
    #[serde(default)]
    rename: Option<String>,
    doc: String,
    fields: Vec<String>,
    #[serde(default)]
    field: Vec<FieldOverride>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldOverride {
    name: String,
    #[serde(default)]
    alias: Option<String>,
    #[serde(default)]
    doc: Option<String>,
}

impl Overrides {
    fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("{} does not parse: {e}", path.display()))
    }
}

// ---------------------------------------------------------------------------
// Resolution

struct Resolved {
    /// In `ClientExt` order - the order of `[[namespace]]` in the overrides.
    namespaces: Vec<Namespace>,
    /// In BFS discovery order from the endpoints, which reads top-down.
    flat_types: Vec<FlatType>,
    /// Grouping modules, one per namespace with models, sorted by module.
    groups: Vec<Group>,
}

struct Namespace {
    kebab: String,
    module: String,
    handler: String,
    accessor_doc: String,
    module_doc: String,
    timeout: Option<ResolvedTimeout>,
    routes: Vec<RouteDecl>,
    endpoints: Vec<Endpoint>,
    tests: Option<ResolvedTests>,
    /// The one version this namespace's routes serve. Multi-version
    /// namespaces will need the handler doc rethought; error until then.
    version: u32,
}

struct ResolvedTimeout {
    name: String,
    secs: u64,
    doc: String,
}

struct RouteDecl {
    const_name: String,
    version: u32,
    resource: String,
    doc: String,
}

struct Endpoint {
    name: String,
    method_name: String,
    /// `Get` / `Post` / `Put` / `Delete` - core's `Method` variant name.
    verb: String,
    route_const: String,
    path_params: Vec<PathParam>,
    body: BodyKind,
    output: RustType,
    timeout: Option<String>,
    endpoint_doc: String,
    method_doc: String,
}

struct PathParam {
    field: String,
    placeholder: String,
}

enum BodyKind {
    None,
    /// `Ok(Some(String::new()))` - what the measured mutating routes without
    /// arguments take.
    EmptyString,
    /// `Ok(Some("{}".to_string()))` - send an empty object, take no
    /// parameters. An override for routes whose optional body we choose not
    /// to model.
    EmptyObject,
    /// The single non-path argument *is* the body, per the client's argument
    /// convention.
    BareArg {
        field: String,
        param_ty: String,
    },
}

/// A Rust-side type for outputs and fields, with the model import it needs.
#[derive(Clone)]
struct RustType {
    /// As written in source (`Vec<Product>`).
    written: String,
    /// The grouped model names this type mentions, as (module, short).
    model_refs: Vec<(String, String)>,
}

impl RustType {
    /// Whether writing this type needs `std::collections::HashMap` in scope.
    fn needs_hashmap(&self) -> bool {
        self.written.contains("HashMap<")
    }
}

struct ResolvedTests {
    name: String,
    cases: Vec<ResolvedCase>,
}

struct ResolvedCase {
    var: String,
    endpoint: String,
    /// (field, value) in declaration order.
    bindings: Vec<(String, String)>,
    expect: String,
}

struct FlatType {
    rust_name: String,
    doc: String,
    fields: Vec<FlatField>,
}

struct FlatField {
    rust_name: String,
    /// The wire spelling, when it differs from `rust_name`.
    rename: Option<String>,
    alias: Option<String>,
    ty: String,
    doc: Option<String>,
}

struct Group {
    module: String,
    doc: String,
    /// (flat name, short name), sorted by flat name.
    exports: Vec<(String, String)>,
}

fn resolve(schema: &Schema, overrides: &Overrides) -> Result<Resolved, String> {
    let type_overrides: BTreeMap<&str, &TypeOverride> = overrides
        .types
        .iter()
        .map(|t| (t.name.as_str(), t))
        .collect();

    let mut models = ModelResolver {
        schema,
        type_overrides,
        emitted: Vec::new(),
        emitted_names: BTreeMap::new(),
    };

    let mut namespaces = Vec::new();
    let mut groups = Vec::new();

    for ns in &overrides.namespace {
        let handler = format!("{}Handler", pascal(&ns.module));
        // The prefix stripped from flat model names is the *client's*
        // namespace spelling, not the module's - `RnetProductRegistryProduct`
        // shortens to `Product` even though the module is `product_registry`.
        let ns_pascal = pascal(&ns.id);
        let mut endpoints = Vec::new();
        let mut routes: Vec<RouteDecl> = Vec::new();
        let mut versions = Vec::new();
        let mut group_refs: Vec<(String, String)> = Vec::new();

        for ep in &ns.endpoint {
            let function = schema
                .functions
                .get(&ep.function)
                .ok_or_else(|| format!("{}: not in the snapshot", ep.function))?;
            let parsed = surface::parse_function(&function.name)
                .ok_or_else(|| format!("{}: not a REST function name", function.name))?;
            if surface::kebab(&parsed.namespace) != ns.id {
                return Err(format!(
                    "{}: belongs to `{}`, listed under `{}`",
                    function.name,
                    surface::kebab(&parsed.namespace),
                    ns.id
                ));
            }

            let arg_names: Vec<&str> = function.arguments.iter().map(|a| a.name.as_str()).collect();
            let segments = surface::derive_segments(&parsed.rest, &arg_names);
            let resource = match &ep.resource {
                Some(spelled) => spelled.clone(),
                None => surface::resource(&segments),
            };

            let path_params: Vec<PathParam> = segments
                .iter()
                .filter_map(|s| match s {
                    Segment::Param { arg_name } => Some(PathParam {
                        field: Segment::field_name(arg_name),
                        placeholder: Segment::placeholder(arg_name),
                    }),
                    Segment::Literal(_) => None,
                })
                .collect();
            let path_arg_names: Vec<&String> = segments
                .iter()
                .filter_map(|s| match s {
                    Segment::Param { arg_name } => Some(arg_name),
                    Segment::Literal(_) => None,
                })
                .collect();

            let name = ep.name.clone().unwrap_or_else(|| parsed.rest.clone());
            let method_name = ep.method_name.clone().unwrap_or_else(|| snake(&name));

            let body_args: Vec<&Argument> = function
                .arguments
                .iter()
                .filter(|a| !path_arg_names.contains(&&a.name))
                .collect();
            let body = resolve_body(&ep.body, &parsed, &body_args, &function.name)?;

            let output =
                models.map_output(&function.returns, &ns.module, &ns_pascal, &function.name)?;
            for reference in &output.model_refs {
                if !group_refs.contains(reference) {
                    group_refs.push(reference.clone());
                }
            }

            // Several endpoints can share one route - the three verbs on a
            // patchline are one resource. The route is declared once, by the
            // first endpoint to name it, and the rest adopt its constant rather
            // than naming one that was never emitted.
            let route_const = match routes
                .iter()
                .find(|r| r.resource == resource && r.version == parsed.version)
            {
                Some(existing) => {
                    if existing.doc != trim_doc(&ep.route_doc) {
                        return Err(format!(
                            "{}: shares a route with `{}` but spells its route_doc differently",
                            function.name, existing.const_name
                        ));
                    }
                    existing.const_name.clone()
                }
                None => {
                    let const_name = snake(&name).to_uppercase();
                    routes.push(RouteDecl {
                        const_name: const_name.clone(),
                        version: parsed.version,
                        resource: resource.clone(),
                        doc: trim_doc(&ep.route_doc),
                    });
                    const_name
                }
            };
            versions.push(parsed.version);

            endpoints.push(Endpoint {
                name,
                method_name,
                verb: parsed.verb.to_string(),
                route_const,
                path_params,
                body,
                output,
                timeout: ep.timeout.clone(),
                endpoint_doc: trim_doc(&ep.endpoint_doc),
                method_doc: trim_doc(&ep.method_doc),
            });
        }

        versions.sort_unstable();
        versions.dedup();
        let &[version] = versions.as_slice() else {
            return Err(format!(
                "namespace `{}` spans versions {versions:?}; the handler doc template assumes one",
                ns.id
            ));
        };

        if let Some(timeout) = &ns.timeout_const {
            let used = endpoints
                .iter()
                .any(|e| e.timeout.as_ref() == Some(&timeout.name));
            if !used {
                return Err(format!(
                    "namespace `{}` declares timeout_const `{}` but no endpoint uses it",
                    ns.id, timeout.name
                ));
            }
        }
        for ep in &endpoints {
            if let Some(timeout) = &ep.timeout {
                if ns.timeout_const.as_ref().map(|t| t.name.as_str()) != Some(timeout.as_str()) {
                    return Err(format!(
                        "endpoint `{}` names timeout `{timeout}`, which its namespace does not \
                         declare",
                        ep.name
                    ));
                }
            }
        }

        let tests = match &ns.tests {
            Some(tests) => Some(resolve_tests(tests, &endpoints, &ns.id)?),
            None => None,
        };

        if !group_refs.is_empty() {
            let doc = ns.models_doc.clone().ok_or_else(|| {
                format!("namespace `{}` has model types but no models_doc", ns.id)
            })?;
            let mut exports: Vec<(String, String)> = group_refs
                .iter()
                .map(|(_, short)| {
                    let flat = models.emitted_names[short].clone();
                    (flat, short.clone())
                })
                .collect();
            // The whole reachable closure is re-exported, not only the types
            // endpoints name directly - a caller holding a `Product` needs to
            // name `Patchline` too.
            for flat in models.reachable_from(&exports) {
                let short = models.short_name(&flat, &ns_pascal);
                if !exports.iter().any(|(f, _)| *f == flat) {
                    exports.push((flat, short));
                }
            }
            exports.sort();
            groups.push(Group {
                module: ns.module.clone(),
                doc: trim_doc(&doc),
                exports,
            });
        } else if ns.models_doc.is_some() {
            return Err(format!(
                "namespace `{}` has a models_doc but no endpoint references a model type",
                ns.id
            ));
        }

        namespaces.push(Namespace {
            kebab: ns.id.clone(),
            module: ns.module.clone(),
            handler,
            accessor_doc: ns.accessor_doc.clone(),
            module_doc: trim_doc(&ns.module_doc),
            timeout: ns.timeout_const.as_ref().map(|t| ResolvedTimeout {
                name: t.name.clone(),
                secs: t.secs,
                doc: trim_doc(&t.doc),
            }),
            routes,
            endpoints,
            tests,
            version,
        });
    }

    groups.sort_by(|a, b| a.module.cmp(&b.module));
    Ok(Resolved {
        namespaces,
        flat_types: models.emitted,
        groups,
    })
}

fn resolve_body(
    body_override: &Option<String>,
    parsed: &surface::ParsedFunction,
    body_args: &[&Argument],
    function: &str,
) -> Result<BodyKind, String> {
    if let Some(kind) = body_override {
        return match kind.as_str() {
            "empty-object" => Ok(BodyKind::EmptyObject),
            "empty-string" => Ok(BodyKind::EmptyString),
            // For a mutating route whose only remaining argument is optional
            // and we choose not to model it. Sending nothing is what leaves the
            // client on its own default.
            "none" => Ok(BodyKind::None),
            other => Err(format!("{function}: unknown body override `{other}`")),
        };
    }
    if parsed.verb == "Get" {
        return Ok(BodyKind::None);
    }
    match body_args {
        [] => Ok(BodyKind::EmptyString),
        [arg] => {
            let param_ty = match (arg.ty.ty.as_str(), arg.ty.element.as_str()) {
                ("vector", "string") => "&'a [String]".to_string(),
                ("string", _) => "&'a str".to_string(),
                ("bool", _) => "bool".to_string(),
                ("int32", _) => "i32".to_string(),
                (ty, element) => {
                    return Err(format!(
                        "{function}: body argument `{}` has unmapped type {ty}<{element}> - \
                         add a body override",
                        arg.name
                    ));
                }
            };
            Ok(BodyKind::BareArg {
                field: Segment::field_name(&arg.name),
                param_ty,
            })
        }
        _ => Err(format!(
            "{function}: several body arguments; state what the body should be with an override"
        )),
    }
}

fn resolve_tests(
    tests: &BindingTests,
    endpoints: &[Endpoint],
    ns_id: &str,
) -> Result<ResolvedTests, String> {
    let mut cases = Vec::new();
    for case in &tests.case {
        let endpoint = endpoints
            .iter()
            .find(|e| e.name == case.endpoint)
            .ok_or_else(|| {
                format!(
                    "tests in `{ns_id}` name endpoint `{}`, which is not declared",
                    case.endpoint
                )
            })?;
        if endpoint.path_params.len() != case.args.len() {
            return Err(format!(
                "test case `{}` supplies {} args; `{}` binds {}",
                case.var,
                case.args.len(),
                case.endpoint,
                endpoint.path_params.len()
            ));
        }
        cases.push(ResolvedCase {
            var: case.var.clone(),
            endpoint: case.endpoint.clone(),
            bindings: endpoint
                .path_params
                .iter()
                .zip(&case.args)
                .map(|(p, v)| (p.field.clone(), v.clone()))
                .collect(),
            expect: case.expect.clone(),
        });
    }
    Ok(ResolvedTests {
        name: tests.name.clone(),
        cases,
    })
}

/// Walks the schema's type graph, emitting each reachable struct type once.
struct ModelResolver<'a> {
    schema: &'a Schema,
    type_overrides: BTreeMap<&'a str, &'a TypeOverride>,
    emitted: Vec<FlatType>,
    /// short/flat bookkeeping: short name -> flat name.
    emitted_names: BTreeMap<String, String>,
}

impl ModelResolver<'_> {
    /// Map a function's return descriptor to a Rust type, pulling any struct
    /// types it references into the flat model set.
    fn map_output(
        &mut self,
        returns: &TypeRef,
        module: &str,
        ns_pascal: &str,
        function: &str,
    ) -> Result<RustType, String> {
        match (returns.ty.as_str(), returns.element.as_str()) {
            ("", _) => Ok(RustType {
                written: "()".to_string(),
                model_refs: Vec::new(),
            }),
            ("vector", element) | ("map", element) => {
                let inner = self.map_output(
                    &TypeRef {
                        ty: element.to_string(),
                        element: String::new(),
                    },
                    module,
                    ns_pascal,
                    function,
                )?;
                // A `map` is always string-keyed: `/help` records only the
                // element type, and the wire form is a JSON object, which has
                // no other kind of key.
                let written = match returns.ty.as_str() {
                    "map" => format!("HashMap<String, {}>", inner.written),
                    _ => format!("Vec<{}>", inner.written),
                };
                Ok(RustType {
                    written,
                    model_refs: inner.model_refs,
                })
            }
            (name, _) => {
                if let Some(primitive) = primitive_type(name) {
                    return Ok(RustType {
                        written: primitive.to_string(),
                        model_refs: Vec::new(),
                    });
                }
                let def = self.schema.types.get(name).ok_or_else(|| {
                    format!("{function}: returns `{name}`, which the snapshot does not describe")
                })?;
                if !def.values.is_empty() {
                    // Enums travel as their variant names; carrying them as
                    // `String` is the tolerance policy - a new variant must
                    // not break deserialization.
                    return Ok(RustType {
                        written: "String".to_string(),
                        model_refs: Vec::new(),
                    });
                }
                let flat = self.emit_struct(name)?;
                let short = self.short_name(&flat, ns_pascal);
                self.emitted_names.insert(short.clone(), flat);
                Ok(RustType {
                    written: short.clone(),
                    model_refs: vec![(module.to_string(), short)],
                })
            }
        }
    }

    /// Emit `name` (subsetted per overrides) and, transitively, every struct
    /// its kept fields reference. Returns the flat Rust name.
    fn emit_struct(&mut self, name: &str) -> Result<String, String> {
        let ov = self.type_overrides.get(name).copied();
        let rust_name = ov
            .and_then(|o| o.rename.clone())
            .unwrap_or_else(|| name.to_string());
        if self.emitted.iter().any(|t| t.rust_name == rust_name) {
            return Ok(rust_name);
        }
        let def = self
            .schema
            .types
            .get(name)
            .ok_or_else(|| format!("model type `{name}` is not in the snapshot"))?;

        // Reserve the slot before recursing so cycles terminate.
        self.emitted.push(FlatType {
            rust_name: rust_name.clone(),
            doc: String::new(),
            fields: Vec::new(),
        });
        let slot = self.emitted.len() - 1;

        let kept: Vec<&FieldDef> = match ov {
            Some(o) => o
                .fields
                .iter()
                .map(|wanted| {
                    def.fields
                        .iter()
                        .find(|f| &f.name == wanted)
                        .ok_or_else(|| {
                            format!("type `{name}` has no field `{wanted}` in the snapshot")
                        })
                })
                .collect::<Result<_, _>>()?,
            None => def.fields.iter().collect(),
        };

        let field_overrides: BTreeMap<&str, &FieldOverride> = ov
            .map(|o| o.field.iter().map(|f| (f.name.as_str(), f)).collect())
            .unwrap_or_default();

        let mut fields = Vec::new();
        for field in kept {
            let rust_field = snake(&field.name);
            let fov = field_overrides.get(field.name.as_str());
            fields.push(FlatField {
                rename: (rust_field != field.name).then(|| field.name.clone()),
                rust_name: rust_field,
                alias: fov.and_then(|o| o.alias.clone()),
                ty: self.map_field_type(&field.ty, name, &field.name)?,
                doc: fov.and_then(|o| o.doc.as_ref()).map(|d| trim_doc(d)),
            });
        }

        self.emitted[slot].doc = ov
            .map(|o| trim_doc(&o.doc))
            .unwrap_or_else(|| format!("`{name}`, as the client describes it."));
        self.emitted[slot].fields = fields;
        Ok(rust_name)
    }

    fn map_field_type(&mut self, ty: &TypeRef, owner: &str, field: &str) -> Result<String, String> {
        match (ty.ty.as_str(), ty.element.as_str()) {
            // Only outputs carry maps so far. Emitting one here would also mean
            // teaching `reachable_from` and the flat header about them, so it
            // stays an error until a field needs it rather than shipping a path
            // nothing has run.
            ("map", element) => Err(format!(
                "`{owner}.{field}`: map fields are not modelled (map<string, {element}>)"
            )),
            ("vector", element) => {
                let inner = self.map_field_type(
                    &TypeRef {
                        ty: element.to_string(),
                        element: String::new(),
                    },
                    owner,
                    field,
                )?;
                Ok(format!("Vec<{inner}>"))
            }
            (name, _) => {
                if let Some(primitive) = primitive_type(name) {
                    return Ok(primitive.to_string());
                }
                let def = self.schema.types.get(name).ok_or_else(|| {
                    format!("`{owner}.{field}`: type `{name}` is not in the snapshot")
                })?;
                if !def.values.is_empty() {
                    return Ok("String".to_string());
                }
                self.emit_struct(name)
            }
        }
    }

    /// The grouped public name: the flat name with the namespace's Pascal
    /// prefix stripped (`RnetProductRegistryProduct` -> `Product`).
    fn short_name(&self, flat: &str, ns_pascal: &str) -> String {
        flat.strip_prefix(ns_pascal)
            .filter(|s| !s.is_empty())
            .unwrap_or(flat)
            .to_string()
    }

    /// Every emitted flat type reachable from `exports` through field types.
    fn reachable_from(&self, exports: &[(String, String)]) -> Vec<String> {
        let mut out = Vec::new();
        let mut queue: Vec<String> = exports.iter().map(|(f, _)| f.clone()).collect();
        while let Some(name) = queue.pop() {
            let Some(def) = self.emitted.iter().find(|t| t.rust_name == name) else {
                continue;
            };
            for field in &def.fields {
                let inner = field.ty.trim_start_matches("Vec<").trim_end_matches('>');
                if self.emitted.iter().any(|t| t.rust_name == inner)
                    && !out.contains(&inner.to_string())
                    && !exports.iter().any(|(f, _)| f == inner)
                {
                    out.push(inner.to_string());
                    queue.push(inner.to_string());
                }
            }
        }
        out
    }
}

fn primitive_type(name: &str) -> Option<&'static str> {
    Some(match name {
        "bool" => "bool",
        "string" => "String",
        "int8" => "i8",
        "int16" => "i16",
        "int32" => "i32",
        "int64" => "i64",
        "uint8" => "u8",
        "uint16" => "u16",
        "uint32" => "u32",
        "uint64" => "u64",
        "float" => "f32",
        "double" => "f64",
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Emission

fn emit(resolved: &Resolved) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    files.push((PathBuf::from("lib.rs"), templates::LIB_RS.to_string()));
    files.push((
        PathBuf::from("namespaces/mod.rs"),
        emit_namespaces_mod(resolved),
    ));
    for ns in &resolved.namespaces {
        let dir = PathBuf::from("namespaces").join(&ns.module);
        files.push((dir.join("routes.rs"), emit_routes(ns)));
        files.push((dir.join("endpoints.rs"), emit_endpoints(ns)));
        files.push((dir.join("mod.rs"), emit_ns_mod(ns)));
    }
    files.push((PathBuf::from("models/mod.rs"), emit_models_mod(resolved)));
    files.push((PathBuf::from("models/flat.rs"), emit_flat(resolved)));
    for group in &resolved.groups {
        files.push((
            PathBuf::from("models").join(format!("{}.rs", group.module)),
            emit_group(group),
        ));
    }
    files
}

/// Prefix every line of `text` with `marker` (`///` or `//!`), at `indent`.
fn doc_block(indent: &str, marker: &str, text: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        if line.is_empty() {
            let _ = writeln!(out, "{indent}{marker}");
        } else {
            let _ = writeln!(out, "{indent}{marker} {line}");
        }
    }
    out
}

fn trim_doc(text: &str) -> String {
    text.trim_end_matches('\n').to_string()
}

fn emit_namespaces_mod(resolved: &Resolved) -> String {
    let mut by_module: Vec<&Namespace> = resolved.namespaces.iter().collect();
    by_module.sort_by(|a, b| a.module.cmp(&b.module));

    let mut out = String::new();
    out.push_str(templates::NAMESPACES_HEADER);
    out.push('\n');
    for ns in &by_module {
        let _ = writeln!(out, "pub mod {};", ns.module);
    }
    out.push('\n');
    out.push_str(
        "use ritoclient_core::client::Client;\n\
         use ritoclient_core::endpoint::EndpointMeta;\n\
         use ritoclient_core::route::Route;\n\n",
    );
    for ns in &by_module {
        let _ = writeln!(out, "use {}::{};", ns.module, ns.handler);
    }
    out.push('\n');

    out.push_str(templates::CLIENT_EXT_DOC);
    out.push_str("pub trait ClientExt {\n");
    for (i, ns) in resolved.namespaces.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let _ = writeln!(out, "    /// {}", ns.accessor_doc);
        let _ = writeln!(out, "    fn {}(&self) -> {}<'_>;", ns.module, ns.handler);
    }
    out.push_str("}\n\n");

    out.push_str("impl ClientExt for Client {\n");
    for (i, ns) in resolved.namespaces.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let _ = writeln!(out, "    fn {}(&self) -> {}<'_> {{", ns.module, ns.handler);
        let _ = writeln!(out, "        {}::new(self)", ns.handler);
        out.push_str("    }\n");
    }
    out.push_str("}\n\n");

    out.push_str(templates::ALL_ROUTES_DOC);
    out.push_str("pub const ALL_ROUTES: &[&[Route]] = &[\n");
    for ns in &by_module {
        let _ = writeln!(out, "    {}::routes::ALL,", ns.module);
    }
    out.push_str("];\n\n");
    out.push_str(templates::ROUTES_FNS);
    out.push('\n');

    out.push_str(templates::ALL_ENDPOINTS_DOC);
    out.push_str("pub const ALL_ENDPOINTS: &[&[EndpointMeta]] = &[\n");
    for ns in &by_module {
        let _ = writeln!(out, "    {}::endpoints::ALL,", ns.module);
    }
    out.push_str("];\n\n");
    out.push_str(templates::ENDPOINTS_FN);
    out.push('\n');

    out.push_str(templates::NAMESPACES_TESTS_HEAD);
    out.push('\n');
    // Exercised on the namespace with the most routes - the deterministic
    // pick that stays meaningful as the surface grows.
    let span_ns = by_module
        .iter()
        .max_by_key(|ns| (ns.routes.len(), std::cmp::Reverse(ns.module.clone())))
        .expect("at least one namespace");
    out.push_str("    #[test]\n");
    out.push_str("    fn a_namespace_lookup_spans_its_versions() {\n");
    out.push_str("        assert_eq!(\n");
    let _ = writeln!(out, "            routes_in(\"{}\").count(),", span_ns.kebab);
    let _ = writeln!(out, "            {}::routes::ALL.len()", span_ns.module);
    out.push_str("        );\n");
    out.push_str("        assert_eq!(routes_in(\"no-such-namespace\").count(), 0);\n");
    out.push_str("    }\n\n");
    out.push_str(templates::NAMESPACES_TESTS_MIDDLE);
    out.push('\n');

    out.push_str(
        "    /// The endpoint tables are emitted beside the impls they mirror, so the\n\
         \x20   /// consts are asserted against the real impls.\n\
         \x20   #[test]\n\
         \x20   fn the_metadata_rows_match_their_endpoint_impls() {\n\
         \x20       fn meta_for(name: &str) -> EndpointMeta {\n\
         \x20           endpoints()\n\
         \x20               .find(|meta| meta.name == name)\n\
         \x20               .unwrap_or_else(|| panic!(\"no EndpointMeta row named {name}\"))\n\
         \x20       }\n\n\
         \x20       fn assert_row<E: Endpoint>(name: &str) {\n\
         \x20           let meta = meta_for(name);\n\
         \x20           assert_eq!(meta.method, E::METHOD, \"{name}\");\n\
         \x20           assert_eq!(meta.route, E::ROUTE, \"{name}\");\n\
         \x20       }\n\n",
    );
    for ns in &by_module {
        for ep in &ns.endpoints {
            let _ = writeln!(
                out,
                "        assert_row::<{}::endpoints::{}>(\"{}\");",
                ns.module, ep.name, ep.name
            );
        }
    }
    out.push_str("    }\n}\n");
    out
}

fn emit_routes(ns: &Namespace) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "//! The routes of `/{}`.", ns.kebab);
    out.push('\n');
    out.push_str("ritoclient_core::routes! {\n");
    let _ = writeln!(out, "    namespace = \"{}\";", ns.kebab);
    for route in &ns.routes {
        out.push('\n');
        out.push_str(&doc_block("    ", "///", &route.doc));
        let _ = writeln!(
            out,
            "    {} = {}, \"{}\";",
            route.const_name, route.version, route.resource
        );
    }
    out.push_str("}\n");
    out
}

fn emit_endpoints(ns: &Namespace) -> String {
    let any_body = ns
        .endpoints
        .iter()
        .any(|e| !matches!(e.body, BodyKind::None));
    let any_bare = ns
        .endpoints
        .iter()
        .any(|e| matches!(e.body, BodyKind::BareArg { .. }));
    let model_refs: Vec<&(String, String)> = {
        let mut refs: Vec<&(String, String)> = ns
            .endpoints
            .iter()
            .flat_map(|e| e.output.model_refs.iter())
            .collect();
        refs.sort();
        refs.dedup();
        refs
    };

    let any_map = ns.endpoints.iter().any(|e| e.output.needs_hashmap());

    let mut out = String::new();
    let _ = writeln!(out, "//! The endpoints of `/{}`.", ns.kebab);
    out.push('\n');
    if any_map {
        out.push_str("use std::collections::HashMap;\n\n");
    }
    if any_bare {
        out.push_str("use ritoclient_core::endpoint::json_body;\n");
    }
    let mut core_items = vec!["Endpoint", "EndpointMeta", "Method"];
    if any_body {
        core_items.push("RequestError");
    }
    core_items.push("Route");
    let _ = writeln!(out, "use ritoclient_core::{{{}}};", core_items.join(", "));
    out.push('\n');
    for (module, short) in &model_refs {
        let _ = writeln!(out, "use crate::models::{module}::{short};");
    }
    if !model_refs.is_empty() {
        out.push('\n');
    }
    out.push_str("use super::routes;\n");

    for ep in &ns.endpoints {
        out.push('\n');
        out.push_str(&doc_block("", "///", &ep.endpoint_doc));

        let borrowed = !ep.path_params.is_empty()
            || matches!(&ep.body, BodyKind::BareArg { param_ty, .. } if param_ty.contains("&'a"));
        let lifetime_decl = if borrowed { "<'a>" } else { "" };
        let lifetime_use = if borrowed { "<'_>" } else { "" };

        let mut fields = Vec::new();
        for param in &ep.path_params {
            fields.push(format!("    pub {}: &'a str,", param.field));
        }
        if let BodyKind::BareArg { field, param_ty } = &ep.body {
            fields.push(format!("    pub {field}: {param_ty},"));
        }

        if fields.is_empty() {
            let _ = writeln!(out, "pub struct {};", ep.name);
        } else {
            let _ = writeln!(out, "pub struct {}{lifetime_decl} {{", ep.name);
            for field in &fields {
                let _ = writeln!(out, "{field}");
            }
            out.push_str("}\n");
        }
        out.push('\n');

        let _ = writeln!(out, "impl Endpoint for {}{lifetime_use} {{", ep.name);
        let _ = writeln!(out, "    type Output = {};", ep.output.written);
        let _ = writeln!(out, "    const METHOD: Method = Method::{};", ep.verb);
        let _ = writeln!(out, "    const ROUTE: Route = routes::{};", ep.route_const);

        if !ep.path_params.is_empty() {
            out.push('\n');
            out.push_str("    fn path(&self) -> String {\n");
            out.push_str("        Self::ROUTE.bind(&[\n");
            for param in &ep.path_params {
                let _ = writeln!(
                    out,
                    "            (\"{}\", self.{}),",
                    param.placeholder, param.field
                );
            }
            out.push_str("        ])\n    }\n");
        }
        match &ep.body {
            BodyKind::None => {}
            BodyKind::EmptyString => {
                out.push('\n');
                out.push_str(
                    "    fn body(&self) -> Result<Option<String>, RequestError> {\n\
                     \x20       Ok(Some(String::new()))\n    }\n",
                );
            }
            BodyKind::EmptyObject => {
                out.push('\n');
                out.push_str(
                    "    fn body(&self) -> Result<Option<String>, RequestError> {\n\
                     \x20       Ok(Some(\"{}\".to_string()))\n    }\n",
                );
            }
            BodyKind::BareArg { field, param_ty } => {
                out.push('\n');
                // `json_body` takes a reference. The borrowed parameter types
                // already are one; the value types (`bool`, `i32`) are not.
                let arg = match param_ty.starts_with('&') {
                    true => format!("self.{field}"),
                    false => format!("&self.{field}"),
                };
                let _ = writeln!(
                    out,
                    "    fn body(&self) -> Result<Option<String>, RequestError> {{\n\
                     \x20       json_body({arg})\n    }}"
                );
            }
        }
        out.push_str("}\n");
    }

    out.push('\n');
    out.push_str("/// Every endpoint this namespace declares, in declaration order.\n");
    out.push_str("pub const ALL: &[EndpointMeta] = &[\n");
    for ep in &ns.endpoints {
        out.push_str("    EndpointMeta {\n");
        let _ = writeln!(out, "        name: \"{}\",", ep.name);
        let _ = writeln!(out, "        method: Method::{},", ep.verb);
        let _ = writeln!(out, "        route: routes::{},", ep.route_const);
        out.push_str("    },\n");
    }
    out.push_str("];\n");
    out
}

fn emit_ns_mod(ns: &Namespace) -> String {
    let any_send = ns.endpoints.iter().any(|e| e.verb != "Get");
    let model_refs: Vec<&(String, String)> = {
        let mut refs: Vec<&(String, String)> = ns
            .endpoints
            .iter()
            .filter(|e| e.verb == "Get")
            .flat_map(|e| e.output.model_refs.iter())
            .collect();
        refs.sort();
        refs.dedup();
        refs
    };

    let mut out = String::new();
    out.push_str(&doc_block("", "//!", &ns.module_doc));
    out.push('\n');
    out.push_str("pub mod endpoints;\npub mod routes;\n\n");
    // Only `Get` handlers write their output type; the rest answer `Response`.
    let any_map = ns
        .endpoints
        .iter()
        .any(|e| e.verb == "Get" && e.output.needs_hashmap());
    if any_map {
        out.push_str("use std::collections::HashMap;\n");
    }
    if ns.timeout.is_some() {
        out.push_str("use std::time::Duration;\n");
    }
    if any_map || ns.timeout.is_some() {
        out.push('\n');
    }
    if any_send {
        out.push_str("use ritoclient_core::client::{Client, RequestError, Response};\n");
    } else {
        out.push_str("use ritoclient_core::client::Client;\n");
    }
    if !model_refs.is_empty() {
        out.push('\n');
        for (module, short) in &model_refs {
            let _ = writeln!(out, "use crate::models::{module}::{short};");
        }
    }
    out.push('\n');

    if let Some(timeout) = &ns.timeout {
        out.push_str(&doc_block("", "///", &timeout.doc));
        let _ = writeln!(
            out,
            "pub const {}: Duration = Duration::from_secs({});",
            timeout.name, timeout.secs
        );
        out.push('\n');
    }

    let _ = writeln!(
        out,
        "/// The `/{}/v{}` namespace. Obtained from",
        ns.kebab, ns.version
    );
    let _ = writeln!(
        out,
        "/// [`ClientExt::{}`](crate::ClientExt::{}).",
        ns.module, ns.module
    );
    let _ = writeln!(out, "pub struct {}<'a> {{", ns.handler);
    out.push_str("    client: &'a Client,\n}\n\n");

    let _ = writeln!(out, "impl<'a> {}<'a> {{", ns.handler);
    out.push_str(
        "    pub(crate) fn new(client: &'a Client) -> Self {\n\
         \x20       Self { client }\n    }\n",
    );

    let mut methods: Vec<&Endpoint> = ns.endpoints.iter().collect();
    methods.sort_by(|a, b| a.method_name.cmp(&b.method_name));
    for ep in methods {
        out.push('\n');
        out.push_str(&doc_block("    ", "///", &ep.method_doc));

        let mut params = String::new();
        for param in &ep.path_params {
            let _ = write!(params, ", {}: &str", param.field);
        }
        if let BodyKind::BareArg { field, param_ty } = &ep.body {
            let _ = write!(params, ", {field}: {}", param_ty.replace("&'a ", "&"));
        }

        let (ret, finisher) = if ep.verb == "Get" {
            (format!("Option<{}>", ep.output.written), "ok")
        } else {
            ("Result<Response, RequestError>".to_string(), "send")
        };

        let mut ctor_fields: Vec<&str> = ep.path_params.iter().map(|p| p.field.as_str()).collect();
        if let BodyKind::BareArg { field, .. } = &ep.body {
            ctor_fields.push(field);
        }
        let ctor = if ctor_fields.is_empty() {
            String::new()
        } else {
            format!(" {{ {} }}", ctor_fields.join(", "))
        };

        let timeout = ep
            .timeout
            .as_ref()
            .map(|t| format!(".timeout({t})"))
            .unwrap_or_default();

        let _ = writeln!(
            out,
            "    pub fn {}(&self{params}) -> {ret} {{",
            ep.method_name
        );
        let _ = writeln!(
            out,
            "        self.client.endpoint(&endpoints::{}{ctor}){timeout}.{finisher}()",
            ep.name
        );
        out.push_str("    }\n");
    }
    out.push_str("}\n");

    if let Some(tests) = &ns.tests {
        out.push('\n');
        out.push_str(
            "#[cfg(test)]\nmod tests {\n    use super::*;\n    use ritoclient_core::Endpoint;\n\n",
        );
        out.push_str("    #[test]\n");
        let _ = writeln!(out, "    fn {}() {{", tests.name);
        for (i, case) in tests.cases.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let _ = writeln!(
                out,
                "        let {} = endpoints::{} {{",
                case.var, case.endpoint
            );
            for (field, value) in &case.bindings {
                let _ = writeln!(out, "            {field}: \"{value}\",");
            }
            out.push_str("        };\n");
            out.push_str("        assert_eq!(\n");
            let _ = writeln!(out, "            {}.path(),", case.var);
            let _ = writeln!(out, "            \"{}\"", case.expect);
            out.push_str("        );\n");
        }
        out.push_str("    }\n}\n");
    }
    out
}

fn emit_models_mod(resolved: &Resolved) -> String {
    let mut out = String::new();
    out.push_str(templates::MODELS_MOD_HEADER);
    out.push('\n');
    out.push_str("mod flat;\n");
    if !resolved.groups.is_empty() {
        out.push('\n');
        for group in &resolved.groups {
            let _ = writeln!(out, "pub mod {};", group.module);
        }
    }
    out
}

fn emit_flat(resolved: &Resolved) -> String {
    let mut out = String::new();
    out.push_str(templates::FLAT_HEADER);
    for ty in &resolved.flat_types {
        out.push('\n');
        out.push_str(&doc_block("", "///", &ty.doc));
        out.push_str("#[derive(Debug, Clone, Default, Deserialize)]\n#[serde(default)]\n");
        let _ = writeln!(out, "pub struct {} {{", ty.rust_name);
        for field in &ty.fields {
            if let Some(doc) = &field.doc {
                out.push_str(&doc_block("    ", "///", doc));
            }
            match (&field.rename, &field.alias) {
                (Some(rename), Some(alias)) => {
                    let _ = writeln!(
                        out,
                        "    #[serde(rename = \"{rename}\", alias = \"{alias}\")]"
                    );
                }
                (Some(rename), None) => {
                    let _ = writeln!(out, "    #[serde(rename = \"{rename}\")]");
                }
                (None, Some(alias)) => {
                    let _ = writeln!(out, "    #[serde(alias = \"{alias}\")]");
                }
                (None, None) => {}
            }
            let _ = writeln!(out, "    pub {}: {},", field.rust_name, field.ty);
        }
        out.push_str("}\n");
    }
    out
}

fn emit_group(group: &Group) -> String {
    let mut out = String::new();
    out.push_str(&doc_block("", "//!", &group.doc));
    out.push('\n');
    out.push_str("pub use super::flat::{\n");
    for (flat, short) in &group.exports {
        let _ = writeln!(out, "    {flat} as {short},");
    }
    out.push_str("};\n");
    out
}

// ---------------------------------------------------------------------------
// Casing

/// `product_registry` -> `ProductRegistry`; `rnet-product-registry` ->
/// `RnetProductRegistry`.
fn pascal(name: &str) -> String {
    name.split(['_', '-'])
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// `NewArgs` -> `new_args`; already-snake names pass through.
fn snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casing_helpers_round_trip_the_module_names() {
        assert_eq!(pascal("product_registry"), "ProductRegistry");
        assert_eq!(pascal("app_args"), "AppArgs");
        assert_eq!(snake("IsLaunchRequestPending"), "is_launch_request_pending");
        assert_eq!(snake("NewArgs"), "new_args");
    }
}
