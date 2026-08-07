//! Record-kind registry, generated at build time from `record_kinds.csv`,
//! the machine-readable transcription of spec §5.3b.
//!
//! Adding a record kind is a public contract change (§5.3b): it MUST first be
//! assigned an existing CEP-0008 §9 `envelope_kind` and tested against the
//! required entity and lineage shape. CEP owns the envelope-kind vocabulary;
//! this registry only maps to it.

include!(concat!(env!("OUT_DIR"), "/record_kinds_gen.rs"));

impl std::fmt::Display for RecordKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_transcription_is_complete_and_unique() {
        // The CSV is the §5.3b transcription; the generated enum must cover
        // every row exactly once, in CSV order.
        let csv = include_str!("record_kinds.csv");
        let kinds: Vec<&str> = csv
            .lines()
            .filter(|l| !l.trim().is_empty())
            .skip(1)
            .map(|l| l.split('|').next().unwrap())
            .collect();
        assert!(!kinds.is_empty());
        let mut seen = std::collections::HashSet::new();
        for kind in &kinds {
            assert!(seen.insert(*kind), "duplicate kind {kind}");
        }
        assert_eq!(kinds.len(), RecordKind::ALL.len());
        for (row, generated) in kinds.iter().zip(RecordKind::ALL.iter()) {
            assert_eq!(*row, generated.as_str());
        }
    }

    #[test]
    fn every_kind_maps_to_a_known_cep_envelope_kind() {
        // AC-SXR-003 half: every registered kind maps to an envelope_kind in
        // CEP-0008 §9's closed vocabulary, as published in the embedded schema.
        let schema = crate::embedded_schema::schema_json();
        let allowed: Vec<&str> = schema["properties"]["envelope_kind"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for kind in RecordKind::ALL {
            let mapped = kind.def().envelope_kind;
            assert!(
                allowed.contains(&mapped),
                "kind {} maps to envelope_kind {mapped} not in CEP §9 vocabulary",
                kind.as_str()
            );
        }
    }

    #[test]
    fn unregistered_kinds_are_rejected() {
        use std::str::FromStr;
        assert!(RecordKind::from_str("repo.enroll").is_err());
        assert!(RecordKind::from_str("ledger.generation_started").is_ok());
        assert!(RecordKind::from_str("").is_err());
        assert!(RecordKind::from_str("REPO.ENROLLED").is_err());
    }

    #[test]
    fn every_row_defines_subject_prefixes_and_predecessors() {
        for kind in RecordKind::ALL {
            let def = kind.def();
            assert!(
                !def.subject_prefixes.is_empty(),
                "{} lacks subject prefixes",
                kind
            );
            assert!(
                !def.predecessors.is_empty(),
                "{} lacks predecessor rules",
                kind
            );
            assert!(
                def.envelope_kind
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_')
            );
        }
    }
}
