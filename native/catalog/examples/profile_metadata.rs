// SPDX-License-Identifier: GPL-3.0-or-later
//! Targeted profiling over explicit real packages; never writes their bytes.
//! Result is private component evidence, not an application speedup claim.
use external_cas_assets::{dbpf::CASP, *};
use external_cas_catalog_cache::{
    CacheBatchLimits, CachePackageUpdate, CachedPart, CachedUnsupported, CatalogCache,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{sync::atomic::AtomicBool, time::Instant};
#[cfg(windows)]
fn cpu_seconds() -> Option<f64> {
    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn GetProcessTimes(
            process: isize,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    let (mut c, mut e, mut k, mut u) = (
        FileTime { low: 0, high: 0 },
        FileTime { low: 0, high: 0 },
        FileTime { low: 0, high: 0 },
        FileTime { low: 0, high: 0 },
    );
    if unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) } == 0 {
        None
    } else {
        Some(
            (((k.high as u64) << 32 | k.low as u64) + ((u.high as u64) << 32 | u.low as u64))
                as f64
                / 1e7,
        )
    }
}
#[cfg(not(windows))]
fn cpu_seconds() -> Option<f64> {
    None
}
fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.
}
fn flush(
    cache: &mut CatalogCache,
    pending: &mut Vec<(PackageIndex, Vec<CachedPart>, Vec<CachedUnsupported>)>,
    cancel: &AtomicBool,
) -> std::result::Result<serde_json::Value, Box<dyn std::error::Error>> {
    let updates = pending
        .iter()
        .map(|(package, rows, unsupported)| CachePackageUpdate {
            package,
            rows,
            unsupported,
        })
        .collect::<Vec<_>>();
    let t = cache.store_packages_with_progress(
        &updates,
        CacheBatchLimits::default(),
        cancel,
        |_| {},
    )?;
    let result = json!({"packages":t.packages,"parts":t.parts,"grams":t.grams,"totalMs":t.total_ms,"leaseAndSourceBeforeMs":t.lease_and_source_before_ms,"beginMs":t.begin_ms,"sqlRowsMs":t.sql_rows_ms,"sourceAfterMs":t.source_after_ms,"commitMs":t.commit_ms});
    pending.clear();
    Ok(result)
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let db = args.next().ok_or("explicit disposable cache path")?;
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(
        args.next().ok_or("explicit private source manifest")?,
    )?)?;
    let batch_size = args
        .next()
        .map(|n| n.parse::<usize>())
        .transpose()?
        .unwrap_or(1);
    if batch_size == 0 || batch_size > 32 {
        return Err("batch_size1..32")?;
    }
    if config["private"] != true {
        return Err("private config required".into());
    }
    let cancel = AtomicBool::new(false);
    let start = Instant::now();
    let cpu_before = cpu_seconds();
    let mut cache = CatalogCache::open(db)?;
    if let Some(kib) = std::env::var("EXTERNAL_CAS_CATALOG_CACHE_KIB")
        .ok()
        .map(|s| s.parse::<u32>())
        .transpose()?
    {
        cache.set_page_cache_budget_kib(kib)?;
    }
    let mut samples = Vec::new();
    let mut pending = Vec::new();
    let mut batches = Vec::new();
    for path in config["packages"].as_array().ok_or("packages")? {
        let source = Instant::now();
        let package = PackageIndex::open(path.as_str().ok_or("package path")?)?;
        let open_ms = ms(source);
        let load = Instant::now();
        let hit = cache.load_package(&package, &cancel)?;
        let load_ms = ms(load);
        if hit.is_some() {
            return Err("targeted cold store profile requires a fresh disposable database".into());
        }
        let stage = Instant::now();
        let mut reader = package.reader()?;
        let reader_ms = ms(stage);
        let stage = Instant::now();
        let mut rows = Vec::new();
        let mut unsupported = Vec::new();
        for (entry_index, entry) in package
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.key.type_id == CASP && !e.deleted())
        {
            match reader
                .read_resource(entry, ReadLimits::default())
                .and_then(|bytes| {
                    Ok((
                        CasPart::parse(&bytes)?,
                        <[u8; 32]>::from(Sha256::digest(&bytes)),
                    ))
                }) {
                Ok((casp, payload_sha256)) => rows.push(CachedPart {
                    entry_index,
                    payload_sha256,
                    casp,
                }),
                Err(error) => unsupported.push(CachedUnsupported {
                    entry_index,
                    message: error.to_string(),
                }),
            }
        }
        let decode_ms = ms(stage);
        drop(reader);
        let store = if batch_size == 1 {
            let t = cache.store_package_profiled_with_progress(
                &package,
                &rows,
                &unsupported,
                &cancel,
                |_| {},
            )?;
            json!({"totalMs":t.total_ms,"validateCompleteMs":t.validate_complete_ms,"leaseMs":t.lease_ms,"sourceBeforeMs":t.source_before_ms,"beginMs":t.begin_ms,"sqlRowsMs":t.sql_rows_ms,"sourceAfterMs":t.source_after_ms,"commitMs":t.commit_ms,"grams":t.grams})
        } else {
            serde_json::Value::Null
        };
        samples.push(json!({"indexEntries":package.entries.len(),"packageBytes":package.file_size,"parts":rows.len(),"unsupported":unsupported.len(),"openMs":open_ms,"coldLoadMissMs":load_ms,"readerMs":reader_ms,"decodeHashParseMs":decode_ms,"store":store}));
        if batch_size != 1 {
            pending.push((package, rows, unsupported));
            if pending.len() >= batch_size {
                batches.push(flush(&mut cache, &mut pending, &cancel)?);
            }
        }
    }
    if !pending.is_empty() {
        batches.push(flush(&mut cache, &mut pending, &cancel)?);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"privateEvidence":true,"kind":"targeted actual-source metadata pipeline; not actual app or synthetic benchmark","batchSize":batch_size,"wallMs":ms(start),"processCpuSeconds":cpu_before.zip(cpu_seconds()).map(|(a,b)|b-a),"samples":samples,"batches":batches})
        )?
    );
    Ok(())
}
