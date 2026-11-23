//! Writes the language schema for the editor (`apps/spikes-ui/src/language/schema.json`); a change shows up in
//! `git diff`.

use std::path::Path;

#[test]
fn write_the_editor_schema() {
    let json = serde_json::to_string_pretty(spikes_dna::schema::RECORDS).unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/spikes-ui/src/language/schema.json");
    std::fs::write(path, json + "\n").unwrap();
}
