// SPDX-License-Identifier: GPL-3.0-or-later
//! No game imports. Deliberately isolated cross-language fixture peer.
use external_cas_link::{protocol::*, *};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Write},
    process::{Command, Stdio},
};
#[test]
#[ignore = "requires explicit CAS_LINK_PYTHON; fixture conformance, not game acceptance"]
fn python_bridge_hmac_and_opaque_snapshot_revision_match_native() {
    let python = std::env::var_os("CAS_LINK_PYTHON").expect("Set explicit Python executable");
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bridge/tests/protocol_oracle.py");
    let identity = SimIdentity {
        save: SaveIdentity {
            slot_id: 4,
            world_id: 8,
            lineage: Sha256::digest(b"sims4-save-guid:12345").into(),
        },
        household_id: 12,
        sim_id: 7,
        form: None,
    };
    let ticket = SelectionTicket {
        session_nonce: [5; 16],
        navigation_epoch: 9,
        request_id: 17,
        identity,
    };
    let mut codec = AuthenticatedCodec::new(
        PairingSecret::from_bytes([3; 32]).unwrap(),
        [5; 16],
        Direction::DesktopToGame,
        1024 * 1024,
    )
    .unwrap();
    let frame = codec
        .encode(&ControlMessage::SnapshotRequest {
            ticket: ticket.clone(),
        })
        .unwrap();
    let mut child = Command::new(python)
        .arg("-B")
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&frame).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ControlMessage::Snapshot {
        ticket: observed,
        snapshot,
        origin_operation,
    } = codec
        .read::<ControlMessage>(&mut Cursor::new(output.stdout))
        .unwrap()
    else {
        panic!("Expected snapshot")
    };
    assert_eq!(observed, ticket);
    assert!(origin_operation.is_none());
    snapshot.validate().unwrap();
    assert_eq!(&*snapshot.fields["outfits"], &[8, 5, 128, 2, 255, 0]);
    assert!(matches!(snapshot.coverage, Coverage::Partial { .. }));
}
