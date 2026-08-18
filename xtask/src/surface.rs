//! The in-scope surface, and how names map onto it.
//!
//! `/help` names functions, not paths. Everything here is the mechanical part
//! of the convention the survey documents (`docs/riot-client-local-api.md`,
//! section 0): `{Verb}{PascalNamespace}V{n}{PascalSegments}`, with `By{Param}`
//! marking a path parameter.

/// The eleven namespaces the survey bolded, in the client's own Pascal names.
///
/// A scoping decision about generation effort, not a safety boundary - the
/// low-level `Client` reaches everything else regardless.
pub const IN_SCOPE: &[&str] = &[
    "ProductLauncher",
    "RnetProductRegistry",
    "Patch",
    "PatchProxy",
    "ProductSession",
    "ProcessControl",
    "Vanguard",
    "Riotclientapp",
    "RiotClientLifecycle",
    "DataStore",
    "LaunchRestriction",
];

/// One function name, decomposed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFunction {
    pub verb: &'static str,
    /// The namespace in the client's Pascal spelling (`RnetProductRegistry`).
    pub namespace: String,
    pub version: u32,
    /// The Pascal remainder after the version (`ProductsByProductIdEligibility`).
    pub rest: String,
}

/// Decompose `PostRnetProductRegistryV4Products` into verb, namespace, version
/// and remainder. `None` for the 37 non-REST builtins (`Help`, `Subscribe`,
/// unversioned names like `GetRiotclientRegionLocale`).
pub fn parse_function(name: &str) -> Option<ParsedFunction> {
    const VERBS: &[&str] = &["Get", "Post", "Put", "Delete", "Head", "Patch"];

    // `Patch` is both a verb and a namespace, so the verb has to be the
    // *shortest* prefix match tried in a fixed order - `PatchChatV1Settings`
    // must read as verb `Patch`, while `PostPatchV1...` reads `Post` + `Patch`.
    let verb = VERBS
        .iter()
        .find(|v| {
            name.strip_prefix(**v)
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()))
        })
        .copied()?;
    let rest = &name[verb.len()..];

    // The namespace runs up to the first `V{digits}` boundary.
    let bytes = rest.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'V' && at + 1 < bytes.len() && bytes[at + 1].is_ascii_digit() && at > 0 {
            let digits_end = bytes[at + 1..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            let version: u32 = rest[at + 1..at + 1 + digits_end].parse().ok()?;
            return Some(ParsedFunction {
                verb,
                namespace: rest[..at].to_string(),
                version,
                rest: rest[at + 1 + digits_end..].to_string(),
            });
        }
        at += 1;
    }
    None
}

/// `RnetProductRegistry` -> `rnet-product-registry`; `Riotclientapp` stays one
/// word because the client itself spells it without dashes - the kebab is a
/// mechanical split on uppercase boundaries, nothing more.
///
/// Known to be imperfect: `GetRiotclientSystemInfoV1BasicInfo` is really
/// mounted at `/riotclient/system-info/v1/basic-info` (a slash where this puts
/// a dash). None of the eleven in-scope namespaces hit that; anything that does
/// gets its spelling from swagger, a recorded probe, or `overrides.toml`.
pub fn kebab(pascal: &str) -> String {
    let mut out = String::with_capacity(pascal.len() + 4);
    for (i, c) in pascal.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// One piece of a derived resource path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// A literal path segment, kebab-cased (`is-launch-request-pending`).
    Literal(String),
    /// A `{camelCase}` placeholder, carrying the argument's kebab name
    /// (`product-id`).
    Param { arg_name: String },
}

impl Segment {
    /// The placeholder spelling used in `Route` resources: camelCase, per the
    /// workspace decision - it never reaches the wire, but the generator and
    /// the hand-written fixture have to agree.
    ///
    /// The client spells argument names both ways - `product-id` under
    /// `product-launcher`, `productId` under `data-store` - so both normalize
    /// through the same word split.
    pub fn placeholder(arg_name: &str) -> String {
        let mut out = String::new();
        for (i, word) in arg_words(arg_name).iter().enumerate() {
            if i == 0 {
                out.push_str(word);
            } else {
                let mut chars = word.chars();
                if let Some(first) = chars.next() {
                    out.push(first.to_ascii_uppercase());
                    out.push_str(chars.as_str());
                }
            }
        }
        out
    }

    /// The Rust field spelling for a path parameter: snake_case.
    pub fn field_name(arg_name: &str) -> String {
        arg_words(arg_name).join("_")
    }
}

/// An argument name as lowercase words: `product-id` and `productId` both
/// split to `["product", "id"]`.
fn arg_words(arg_name: &str) -> Vec<String> {
    let mut words = Vec::new();
    for part in arg_name.split('-') {
        for word in split_words(part) {
            words.push(word.to_ascii_lowercase());
        }
    }
    words
}

/// Derive a resource path from the Pascal remainder and the function's
/// argument names.
///
/// `ProductsByProductIdPatchlinesByPatchlineId` with arguments `product-id`,
/// `patchline-id` becomes `products/{productId}/patchlines/{patchlineId}`.
///
/// A run of words between parameters becomes **one** kebab segment
/// (`IsLaunchRequestPending` -> `is-launch-request-pending`). That is the
/// documented ambiguity of the convention: `...ByInstallIdStatusPatch` is
/// really `.../status/patch`, two segments, and nothing in the name says so.
/// Swagger's spelling wins where it has one, a recorded probe next; this
/// derivation is the fallback, per the codegen plan.
pub fn derive_segments(rest: &str, arg_names: &[&str]) -> Vec<Segment> {
    let words = split_words(rest);
    let mut segments = Vec::new();
    let mut run: Vec<&str> = Vec::new();
    let mut at = 0;

    while at < words.len() {
        let mut matched = None;
        if words[at] == "By" {
            // Match the longest argument name spelled out after `By`,
            // comparing lowercased words so both of the client's argument
            // spellings (`product-id`, `productId`) line up with the name.
            for arg in arg_names {
                let wanted = arg_words(arg);
                let end = at + 1 + wanted.len();
                if end <= words.len()
                    && words[at + 1..end]
                        .iter()
                        .map(|w| w.to_ascii_lowercase())
                        .eq(wanted.iter().cloned())
                    && matched.as_ref().is_none_or(|(_, len)| wanted.len() > *len)
                {
                    matched = Some((arg.to_string(), wanted.len()));
                }
            }
        }

        match matched {
            Some((arg_name, len)) => {
                if !run.is_empty() {
                    segments.push(Segment::Literal(kebab(&run.join(""))));
                    run.clear();
                }
                segments.push(Segment::Param { arg_name });
                at += 1 + len;
            }
            None => {
                run.push(words[at]);
                at += 1;
            }
        }
    }
    if !run.is_empty() {
        segments.push(Segment::Literal(kebab(&run.join(""))));
    }
    segments
}

/// Render derived segments as a `Route` resource string.
pub fn resource(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|s| match s {
            Segment::Literal(text) => text.clone(),
            Segment::Param { arg_name } => format!("{{{}}}", Segment::placeholder(arg_name)),
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Split Pascal text at uppercase boundaries: `NewArgs` -> `["New", "Args"]`.
fn split_words(pascal: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let mut start = 0;
    for (i, c) in pascal.char_indices() {
        if i > 0 && c.is_ascii_uppercase() {
            words.push(&pascal[start..i]);
            start = i;
        }
    }
    if start < pascal.len() {
        words.push(&pascal[start..]);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decomposes_the_survey_examples() {
        let parsed = parse_function("GetPatchV1InstallsByInstallIdStatusPatch").unwrap();
        assert_eq!(parsed.verb, "Get");
        assert_eq!(parsed.namespace, "Patch");
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.rest, "InstallsByInstallIdStatusPatch");

        let parsed =
            parse_function("PostProductLauncherV1ProductsByProductIdPatchlinesByPatchlineId")
                .unwrap();
        assert_eq!(parsed.namespace, "ProductLauncher");
        assert_eq!(parsed.rest, "ProductsByProductIdPatchlinesByPatchlineId");
    }

    /// `Patch` the verb and `Patch` the namespace must not be confused.
    #[test]
    fn patch_is_both_a_verb_and_a_namespace() {
        assert_eq!(parse_function("PatchChatV1Settings").unwrap().verb, "Patch");
        assert_eq!(
            parse_function("PostPatchV1Something").unwrap().namespace,
            "Patch"
        );
    }

    /// The builtins and the unversioned names are not REST functions.
    #[test]
    fn non_rest_names_parse_to_none() {
        assert!(parse_function("Help").is_none());
        assert!(parse_function("Subscribe").is_none());
        assert!(parse_function("GetRiotclientRegionLocale").is_none());
    }

    #[test]
    fn kebabs_match_the_clients_spellings() {
        assert_eq!(kebab("RnetProductRegistry"), "rnet-product-registry");
        assert_eq!(kebab("Riotclientapp"), "riotclientapp");
        assert_eq!(kebab("RiotClientLifecycle"), "riot-client-lifecycle");
        assert_eq!(kebab("DataStore"), "data-store");
    }

    #[test]
    fn derives_the_launch_route() {
        let segments = derive_segments(
            "ProductsByProductIdPatchlinesByPatchlineId",
            &["product-id", "patchline-id"],
        );
        assert_eq!(
            resource(&segments),
            "products/{productId}/patchlines/{patchlineId}"
        );
    }

    #[test]
    fn a_word_run_is_one_kebab_segment() {
        let segments = derive_segments("IsLaunchRequestPending", &[]);
        assert_eq!(resource(&segments), "is-launch-request-pending");
    }

    /// The documented failure mode, pinned so nobody mistakes the derivation
    /// for the truth: the client serves `installs/{installId}/status/patch`,
    /// and the name alone cannot say where `StatusPatch` splits.
    #[test]
    fn the_status_patch_ambiguity_stays_ambiguous() {
        let segments = derive_segments("InstallsByInstallIdStatusPatch", &["install-id"]);
        assert_eq!(resource(&segments), "installs/{installId}/status-patch");
    }

    #[test]
    fn placeholders_are_camel_and_fields_are_snake() {
        assert_eq!(Segment::placeholder("product-id"), "productId");
        assert_eq!(Segment::field_name("product-id"), "product_id");
        // The client's other argument spelling normalizes identically.
        assert_eq!(Segment::placeholder("productId"), "productId");
        assert_eq!(Segment::field_name("productId"), "product_id");
    }

    /// `data-store` spells its arguments camelCase where `product-launcher`
    /// spells them kebab; the derivation accepts both.
    #[test]
    fn camel_case_argument_names_still_mark_parameters() {
        let segments = derive_segments(
            "ProductSettingsProductsByProductIdPatchlinesByPatchlineId",
            &["productId", "patchlineId"],
        );
        assert_eq!(
            resource(&segments),
            "product-settings-products/{productId}/patchlines/{patchlineId}"
        );
    }
}
