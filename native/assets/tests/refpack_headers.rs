// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_assets::{Error, PackageIndex, ReadLimits, dbpf::decode_refpack};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
};

fn fixture(flags: u8) -> Vec<u8> {
    let mut out = vec![flags, 0xfb];
    if flags & 0x80 != 0 {
        out.push(0);
    }
    out.extend([0, 0, 5, 0xe0]);
    out.extend(b"STBL");
    out.extend([0xfd, 5]);
    out
}

#[test]
fn qualified_signature_high_flags_and_size_width_preserve_strict_stream_checks() {
    let cancel = AtomicBool::new(false);
    for flags in [0x10, 0x30, 0x50, 0x70, 0x90, 0xb0, 0xd0, 0xf0] {
        let stream = fixture(flags);
        assert_eq!(
            decode_refpack(&stream, 5, ReadLimits::default(), &cancel).unwrap(),
            b"STBL\x05"
        );
        let mut trailing = stream.clone();
        trailing.push(0);
        assert!(decode_refpack(&trailing, 5, ReadLimits::default(), &cancel).is_err());
        for at in 0..stream.len() {
            assert!(decode_refpack(&stream[..at], 5, ReadLimits::default(), &cancel).is_err());
        }
        assert!(decode_refpack(&stream, 6, ReadLimits::default(), &cancel).is_err());
        assert!(matches!(
            decode_refpack(
                &stream,
                5,
                ReadLimits {
                    max_resource_bytes: 4
                },
                &cancel
            ),
            Err(Error::Budget { .. })
        ));
        assert!(matches!(
            decode_refpack(&stream, 5, ReadLimits::default(), &AtomicBool::new(true)),
            Err(Error::Cancelled)
        ));
    }
    for flags in [0, 0x11, 0x51, 0x91, 0xd1, 0x12, 0x20, 0x40, 0xff] {
        assert!(decode_refpack(&fixture(flags), 5, ReadLimits::default(), &cancel).is_err());
    }
    let invalid_backreference = [0x50, 0xfb, 0, 0, 3, 0, 0, 0xfc];
    assert!(decode_refpack(&invalid_backreference, 3, ReadLimits::default(), &cancel).is_err());
}

#[test]
#[ignore = "explicit read-only installed package and unchanged independent Gibbed source harness required"]
fn installed_refpack_payloads_match_independent_source_reader() {
    let path = std::env::var_os("EXTERNAL_CAS_REFPACK_SOURCE").expect("explicit source package");
    let oracle = std::env::var_os("EXTERNAL_CAS_REFPACK_ORACLE")
        .expect("explicit independent source harness");
    assert!(std::path::Path::new(&oracle).is_absolute());
    let package = PackageIndex::open(path).unwrap();
    let cancel = AtomicBool::new(false);
    let mut file = std::fs::File::open(&package.path).unwrap();
    let mut rows = Vec::new();
    for entry in package
        .entries
        .iter()
        .filter(|e| !e.deleted() && e.compression == 0xffff)
    {
        assert!(entry.stored_size < 16 * 1024 * 1024);
        file.seek(SeekFrom::Start(entry.offset)).unwrap();
        let mut stored = vec![0; entry.stored_size as usize];
        file.read_exact(&mut stored).unwrap();
        package.revalidate(&cancel).unwrap();
        let decoded = package
            .read_resource(
                entry,
                ReadLimits {
                    max_resource_bytes: 16 * 1024 * 1024,
                },
            )
            .unwrap();
        let mut child = Command::new(&oracle)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&stored).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let reference: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let hash = format!("{:x}", Sha256::digest(&decoded));
        assert_eq!(reference["bytes"].as_u64(), Some(decoded.len() as u64));
        assert_eq!(reference["sha256"].as_str(), Some(hash.as_str()));
        rows.push(serde_json::json!({"key":entry.key.to_string(),"header_flags":stored[0],"bytes":decoded.len(),"payload_sha256":hash,"independent_match":true}));
    }
    assert!(!rows.is_empty());
    package.revalidate(&cancel).unwrap();
    if let Some(receipt) = std::env::var_os("EXTERNAL_CAS_REFPACK_RECEIPT") {
        std::fs::write(receipt, serde_json::to_vec_pretty(&serde_json::json!({"private":true,"game_contact":false,"source_mutations":false,"rows":rows})).unwrap()).unwrap();
    }
    eprintln!(
        "{} installed RefPack resources matched the independent source reader",
        rows.len()
    );
}
