// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_assets::{CasPart, PackageIndex, ReadLimits, dbpf::CASP};
use external_cas_catalog_cache::{CachedPart, CachedUnsupported, CatalogCache};
use sha2::{Digest, Sha256};
use std::{sync::atomic::AtomicBool, time::Instant};
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let db = args
        .next()
        .ok_or("pass absolute private cache path then explicit package paths")?;
    let paths = args.collect::<Vec<_>>();
    if paths.is_empty() {
        return Err("no package paths".into());
    }
    let cancel = AtomicBool::new(false);
    let mut cache = CatalogCache::open(db)?;
    let mut evidence = Vec::new();
    let mut fingerprints = Vec::new();
    let mut expected_search = Vec::new();
    for path in paths {
        let package = PackageIndex::open(&path)?;
        let mut runs = Vec::new();
        let mut baseline = None;
        for pass in 0..2 {
            let started = Instant::now();
            let loaded = cache.load_package(&package, &cancel)?;
            let hit = loaded.is_some();
            let mut parsed = 0;
            let loaded = if let Some(loaded) = loaded {
                loaded
            } else {
                let mut reader = package.reader()?;
                let mut rows = Vec::new();
                let mut unsupported = Vec::new();
                for (entry_index, entry) in package.entries.iter().enumerate() {
                    if entry.key.type_id != CASP || entry.deleted() {
                        continue;
                    }
                    match reader.read_resource(entry, ReadLimits::default()) {
                        Ok(bytes) => match CasPart::parse(&bytes) {
                            Ok(casp) => {
                                parsed += 1;
                                rows.push(CachedPart {
                                    entry_index,
                                    payload_sha256: Sha256::digest(&bytes).into(),
                                    casp,
                                });
                            }
                            Err(e) => unsupported.push(CachedUnsupported {
                                entry_index,
                                message: e.to_string(),
                            }),
                        },
                        Err(e) => unsupported.push(CachedUnsupported {
                            entry_index,
                            message: e.to_string(),
                        }),
                    }
                }
                cache.store_package(&package, &rows, &unsupported, &cancel)?;
                external_cas_catalog_cache::CachedPackage { rows, unsupported }
            };
            let identity = loaded
                .rows
                .iter()
                .map(|row| {
                    Ok((
                        row.entry_index,
                        row.payload_sha256,
                        serde_json::to_value(&row.casp)?,
                    ))
                })
                .collect::<Result<Vec<_>, serde_json::Error>>()?;
            if let Some((previous, unsupported)) = &baseline {
                if previous != &identity || unsupported != &loaded.unsupported {
                    return Err("cache changed real metadata or unsupported status".into());
                }
            } else {
                baseline = Some((identity, loaded.unsupported.clone()));
            }
            runs.push(serde_json::json!({"pass":pass,"cacheHit":hit,"parsedThisPass":parsed,"rows":loaded.rows.len(),"unsupported":loaded.unsupported.len(),"identicalMetadata":true,"elapsedMs":started.elapsed().as_secs_f64()*1000.}));
            if pass == 1 {
                for row in &loaded.rows {
                    let key = package.entries[row.entry_index].key;
                    let normalized = format!(
                        "{} {} {}",
                        row.casp.name,
                        package.path.file_name().unwrap().to_string_lossy(),
                        key
                    )
                    .to_lowercase();
                    expected_search.push((
                        package.path.clone(),
                        row.entry_index,
                        key,
                        row.payload_sha256,
                        normalized,
                        row.casp.body_type,
                    ));
                }
            }
        }
        package.revalidate(&cancel)?;
        evidence.push(serde_json::json!({"packageLabel":package.path.file_name().unwrap().to_string_lossy(),"sourceBytes":package.file_size,"indexRows":package.entries.len(),"runs":runs}));
        fingerprints.push(package.fingerprint());
    }
    let scope_digest = cache.bind_scope("actual-source-qualification", &fingerprints, &cancel)?;
    let mut queries = Vec::new();
    for query in [
        "",
        "a",
        "ha",
        "HAIR",
        "dress",
        "solid black",
        "package",
        "ECA56318",
    ] {
        for body in [None, Some(2)] {
            let normalized = query.to_lowercase();
            let words = normalized.split_whitespace().collect::<Vec<_>>();
            let expected = expected_search
                .iter()
                .filter(|r| body.is_none_or(|b| r.5 == b) && words.iter().all(|w| r.4.contains(w)))
                .map(|r| (r.0.clone(), r.1, r.2, r.3))
                .collect::<std::collections::BTreeSet<_>>();
            let hits = cache.search_scoped(
                "actual-source-qualification",
                query,
                body,
                0,
                usize::MAX,
                &cancel,
            )?;
            let actual = hits
                .iter()
                .map(|r| {
                    (
                        r.package_path.clone(),
                        r.entry_index,
                        r.key,
                        r.payload_sha256,
                    )
                })
                .collect::<std::collections::BTreeSet<_>>();
            if expected != actual {
                return Err("real scoped search differs from discovered source metadata".into());
            }
            queries.push(serde_json::json!({"query":query,"body":body,"rows":hits.len(),"exactScopedResults":true}));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"scope":"actual explicit source cache component; not app startup or UI performance","packages":evidence,"search":{"qualifiedPackages":fingerprints.len(),"sourceRows":expected_search.len(),"scopeDigest":scope_digest.iter().map(|n|format!("{n:02x}")).collect::<String>(),"queries":queries}})
        )?
    );
    Ok(())
}
