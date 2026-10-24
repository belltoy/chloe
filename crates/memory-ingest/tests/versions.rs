//! Hook payloads of different agent versions (B-14). Every `tests/fixtures/<dir>/**.json`
//! holds a payload and what it must turn into: an event of a given kind, "ignored" (an event
//! we do not capture) or an error naming the missing core field (counted by the caller).
//! `<agent>/<version>/` files are real payloads or copies of them; `synthetic/` ones are made up
//! to test tolerance. To support a new agent version, drop its captured payloads in a new
//! directory; nothing else changes.

use std::path::Path;

use memory_ingest::hooks::{CLAUDE, CODEX, Error, parse_hook};
use serde_json::Value;
use time::OffsetDateTime;

fn files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            files(&p, out);
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
}

#[test]
fn every_fixture_parses_as_expected() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut all = vec![];
    files(&root, &mut all);
    all.sort();
    assert!(all.len() >= 20, "fixtures are missing: {}", all.len());
    let mut versions = std::collections::BTreeSet::new();
    for path in &all {
        let name = path.strip_prefix(&root).unwrap().display().to_string();
        if let Some(dir) = name.rsplit_once('/').map(|x| x.0) {
            versions.insert(dir.to_string());
        }
        let f: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let spec = match f["spec"].as_str().unwrap() {
            "claude" => &CLAUDE,
            "codex" => &CODEX,
            s => panic!("{name}: unknown spec {s}"),
        };
        let got = parse_hook(
            spec,
            f["payload"].to_string().as_bytes(),
            OffsetDateTime::UNIX_EPOCH,
        );
        let want = &f["expect"];
        if let Some(field) = want.get("error") {
            let field = field.as_str().unwrap();
            match got {
                Err(Error::Missing(m)) => assert_eq!(m, field, "{name}"),
                other => panic!("{name}: wanted missing `{field}`, got {other:?}"),
            }
        } else if want.get("ignored").is_some() {
            assert!(matches!(got, Ok(None)), "{name}: {got:?}");
        } else {
            let e = got
                .unwrap_or_else(|e| panic!("{name}: {e}"))
                .unwrap_or_else(|| panic!("{name}: ignored"));
            assert_eq!(e.kind.as_str(), want["kind"].as_str().unwrap(), "{name}");
            if let Some(t) = want.get("text") {
                assert_eq!(e.text.as_deref(), t.as_str(), "{name}");
            }
            if let Some(t) = want.get("tool") {
                assert_eq!(e.tool_name.as_deref(), t.as_str(), "{name}");
            }
            if let Some(id) = want.get("native_event_id") {
                assert_eq!(e.native_event_id.as_deref(), id.as_str(), "{name}");
            }
            // Command output never reaches the stored event.
            assert!(
                !serde_json::to_string(&e).unwrap().contains("SECRET-OUTPUT"),
                "{name}"
            );
        }
    }
    // A captured version per agent is the point of the directory layout.
    for v in [
        "claude_code/2.1.309",
        "claude_code/2.1.310",
        "codex/0.164.0",
        "synthetic",
    ] {
        assert!(versions.contains(v), "no fixtures for {v}");
    }
}
