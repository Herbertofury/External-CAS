// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_link::{LinkError, protocol::*};
use std::{
    io::Cursor,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};
fn codec(direction: Direction) -> AuthenticatedCodec {
    AuthenticatedCodec::new(
        PairingSecret::from_bytes([3; 32]).unwrap(),
        [5; 16],
        direction,
        1024,
    )
    .unwrap()
}
#[test]
fn authentication_tampering_replay_direction_and_size_are_rejected() {
    let mut sender = codec(Direction::DesktopToGame);
    let frame = sender.encode(&ControlMessage::Disconnect).unwrap();
    let mut receiver = codec(Direction::GameToDesktop);
    assert!(matches!(
        receiver
            .read::<ControlMessage>(&mut Cursor::new(&frame))
            .unwrap(),
        ControlMessage::Disconnect
    ));
    assert!(matches!(
        receiver.read::<ControlMessage>(&mut Cursor::new(&frame)),
        Err(LinkError::Replay)
    ));
    let mut forged = frame.clone();
    forged[39] ^= 1;
    assert!(matches!(
        codec(Direction::GameToDesktop).read::<ControlMessage>(&mut Cursor::new(forged)),
        Err(LinkError::Authentication)
    ));
    assert!(matches!(
        codec(Direction::DesktopToGame).read::<ControlMessage>(&mut Cursor::new(&frame)),
        Err(LinkError::Authentication)
    ));
    let mut old = frame.clone();
    old[10] ^= 1;
    assert!(matches!(
        codec(Direction::GameToDesktop).read::<ControlMessage>(&mut Cursor::new(old)),
        Err(LinkError::Authentication)
    ));
    let mut huge = frame.clone();
    huge[35..39].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        codec(Direction::GameToDesktop).read::<ControlMessage>(&mut Cursor::new(huge)),
        Err(LinkError::Budget(_))
    ));
    for n in 0..frame.len() {
        assert!(
            codec(Direction::GameToDesktop)
                .read::<ControlMessage>(&mut Cursor::new(&frame[..n]))
                .is_err()
        );
    }
}
#[test]
fn local_endpoint_is_explicit_and_remote_addresses_are_refused() {
    assert!(
        connect_loopback(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 43210),
            Duration::from_millis(10)
        )
        .is_err()
    );
    let listener = bind_loopback(0).unwrap();
    assert!(listener.local_addr().unwrap().ip().is_loopback());
    assert!(!format!("{:?}", PairingSecret::from_bytes([3; 32]).unwrap()).contains("3, 3"));
}
