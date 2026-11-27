//! The older form converts to the current one: every v1 test network migrates, and the result parses.

use std::path::Path;

#[test]
fn every_v1_network_migrates_and_parses() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/old");
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/migrated");
    // (A fresh checkout has no such folder yet.)
    std::fs::create_dir_all(&out).unwrap();
    let mut failures = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries.iter().filter(|p| p.extension().is_some_and(|e| e == "boo")) {
        let text = std::fs::read_to_string(path).unwrap();
        match spikes_dna::migrate(&text) {
            Ok(new) => {
                std::fs::write(out.join(path.file_name().unwrap()), &new).unwrap();
                if let Err(e) = spikes_dna::parse(&new) {
                    failures.push(format!(
                        "{}: doesn't parse after migrating: {:?}",
                        path.display(),
                        &e[..e.len().min(3)]
                    ));
                }
            }
            Err(e) => failures.push(format!("{}: {:?}", path.display(), &e[..e.len().min(3)])),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
