//! The half of the tolerance policy `derive` cannot supply.
//!
//! `#[serde(default)]` covers a key that is *absent*. It does nothing for one
//! that is present and `null`, which is a different thing on the wire and just
//! as routine - `/product-session` reports `"exitReason": null` for every
//! session that has not ended yet, on a field documented as a string.
//!
//! Hand-written, and beside [`super::flat`] rather than inside it, because that
//! file is generator output. What a regeneration emits is a `deserialize_with`
//! pointing here.

use serde::{Deserialize, Deserializer};

/// Read a value that may arrive as `null`, falling back to its default.
///
/// Between this and `#[serde(default)]` a field always deserializes, whatever
/// the client leaves out or nulls out. That matters more here than it looks:
/// these namespaces have no schema, one unexpected `null` fails the whole
/// response, and the caller sees an absent record rather than a parse error -
/// so the symptom is a feature that quietly does nothing.
///
/// The default is a real answer rather than a placeholder. An empty string is
/// what the client already sends for "nothing here" elsewhere, and the
/// `ritoclient` crate's readers are written against that: an empty
/// `install_full_path` means not installed, and an empty `exit_reason` means no
/// evidence either way.
pub(super) fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Debug, Default, Deserialize)]
    #[serde(default)]
    struct Probe {
        #[serde(deserialize_with = "super::or_default")]
        name: String,
        #[serde(deserialize_with = "super::or_default")]
        count: i64,
        #[serde(deserialize_with = "super::or_default")]
        items: Vec<String>,
    }

    /// The case that took the session watcher out: a key that is present, typed
    /// as a string, and null.
    #[test]
    fn a_null_reads_as_the_default() {
        let probe: Probe =
            serde_json::from_str(r#"{"name":null,"count":null,"items":null}"#).unwrap();

        assert_eq!(probe.name, "");
        assert_eq!(probe.count, 0);
        assert!(probe.items.is_empty());
    }

    /// The absent case still belongs to `#[serde(default)]`, and adding a
    /// `deserialize_with` must not have taken it away.
    #[test]
    fn an_absent_key_still_defaults() {
        let probe: Probe = serde_json::from_str("{}").unwrap();

        assert_eq!(probe.name, "");
        assert_eq!(probe.count, 0);
        assert!(probe.items.is_empty());
    }

    /// Tolerating null must not start tolerating the wrong type - a number
    /// where a string belongs is a shape change worth failing on, because
    /// nothing sensible can be read from it.
    #[test]
    fn a_wrong_type_still_fails() {
        assert!(serde_json::from_str::<Probe>(r#"{"name":42}"#).is_err());
    }

    #[test]
    fn a_present_value_survives() {
        let probe: Probe =
            serde_json::from_str(r#"{"name":"live","count":7,"items":["a"]}"#).unwrap();

        assert_eq!(probe.name, "live");
        assert_eq!(probe.count, 7);
        assert_eq!(probe.items, vec!["a".to_string()]);
    }
}
