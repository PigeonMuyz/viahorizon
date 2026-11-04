// SPDX-License-Identifier: GPL-3.0-or-later
use viahorizon_native::{vfp_core::put_varint, vfp_mining::Mining};
fn action(kind: i32, direction: u8) -> Vec<u8> {
    let mut out = Vec::new();
    put_varint(&mut out, kind);
    out.extend([0, 0, 0, 0, 0, 0, 0, 42]);
    out.push(direction);
    put_varint(&mut out, 0);
    out
}
fn tick(mining: &mut Mining) -> Option<(i32, Vec<u8>)> {
    mining.serverbound("PUNCH", &[]).unwrap();
    mining.serverbound("CLIENT_TICK_END", &[]).unwrap()
}
#[test]
fn first_tick_is_silent_then_sounds_play_every_four_ticks() {
    let mut mining = Mining::default();
    mining.serverbound("PLAYER_ACTION", &action(0, 2)).unwrap();
    assert!(tick(&mut mining).is_none());
    for step in 1..=17 {
        let (_, event) = tick(&mut mining).unwrap();
        assert_eq!(
            i32::from_be_bytes(event[..4].try_into().unwrap()),
            if step % 4 == 0 { 2020 } else { 2019 }
        );
        assert_eq!(&event[4..12], &[0, 0, 0, 0, 0, 0, 0, 42]);
        assert_eq!(&event[12..], &[0, 0, 0, 2, 0]);
    }
}
#[test]
fn changing_direction_keeps_progress_and_does_not_emit_an_extra_effect() {
    let mut mining = Mining::default();
    mining.serverbound("PLAYER_ACTION", &action(0, 2)).unwrap();
    tick(&mut mining);
    assert!(
        mining
            .serverbound("PLAYER_ACTION", &action(1, 5))
            .unwrap()
            .is_none()
    );
    let (_, event) = tick(&mut mining).unwrap();
    assert_eq!(&event[12..], &[0, 0, 0, 5, 0]);
}
#[test]
fn abort_stop_and_missing_continuation_clear_the_mining_session() {
    for stop in [2, 3] {
        let mut mining = Mining::default();
        mining.serverbound("PLAYER_ACTION", &action(0, 0)).unwrap();
        tick(&mut mining);
        mining
            .serverbound("PLAYER_ACTION", &action(stop, 0))
            .unwrap();
        assert!(tick(&mut mining).is_none());
    }
    let mut mining = Mining::default();
    mining.serverbound("PLAYER_ACTION", &action(0, 0)).unwrap();
    tick(&mut mining);
    mining.serverbound("CLIENT_TICK_END", &[]).unwrap();
    assert!(tick(&mut mining).is_none());
}
#[test]
fn malformed_inputs_cannot_change_mining_state() {
    let mut mining = Mining::default();
    mining.serverbound("PLAYER_ACTION", &action(0, 2)).unwrap();
    let saved = mining.clone();
    assert!(mining.serverbound("PLAYER_ACTION", &action(1, 6)).is_err());
    assert_eq!(mining, saved);
    assert!(mining.serverbound("PUNCH", &[1]).is_err());
    assert_eq!(mining, saved);
    for n in 0..action(0, 2).len() {
        assert!(
            mining
                .serverbound("PLAYER_ACTION", &action(0, 2)[..n])
                .is_err()
        );
        assert_eq!(mining, saved);
    }
}
