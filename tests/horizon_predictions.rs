// SPDX-License-Identifier: GPL-3.0-or-later
use horizon_protocol::{
    buf::{Codec, Reader, Writer},
    item::{Component, Hashed, Slot},
    packets::play,
    vfp_core as core, vfp_hash as hash, vfp_items as items,
    vfp_prediction::{self as prediction, Predictions},
    vfp_tables as t, vfp_wire as wire,
};
use horizon_world::nbt::Tag;
fn convert(bytes: &[u8], forward: bool) -> Vec<u8> {
    let mut r = Reader::new(bytes);
    let mut w = Writer::new();
    items::stack_262(&mut r, &mut w, forward, false).unwrap();
    assert_eq!(r.remaining(), 0);
    w.data
}
fn old_stack(color: i32, custom: Option<&Tag>) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(1);
    w.varint(72);
    w.varint(if custom.is_some() { 2 } else { 1 });
    w.varint(0);
    if let Some(tag) = custom {
        w.varint(0);
        w.nbt(tag);
    }
    w.varint(45);
    w.i32(color);
    w.data
}
fn observe(cache: &mut Predictions, old: &[u8]) -> Hashed {
    let bytes = convert(old, true);
    let stack = Slot::read(&mut Reader::new(&bytes)).unwrap().unwrap();
    let Some(Component::Nbt(tag)) = stack.component(0) else {
        panic!()
    };
    let predicted = Hashed {
        item: stack.item,
        count: stack.count,
        added: vec![(0, prediction::nbt(tag).unwrap())],
        removed: Vec::new(),
    };
    cache
        .observe(&[(
            wire::packet_id(t::NEW_CLIENTBOUND, "SET_CURSOR_ITEM").unwrap(),
            bytes,
        )])
        .unwrap();
    predicted
}
#[test]
fn nbt_hash_matches_the_upstream_compound_fixture_without_utf16_loss() {
    let tag = Tag::compound([
        ("id", Tag::Int(1)),
        ("type", Tag::String("minecraft:stick".into())),
        ("wow", Tag::compound([] as [(&str, Tag); 0])),
        ("list", Tag::List(vec![Tag::Float(0.3f32.to_bits())])),
        ("emptylist", Tag::List(Vec::new())),
    ]);
    assert_eq!(prediction::nbt(&tag).unwrap(), -859008863);
    assert_eq!(
        prediction::nbt(&Tag::String(horizon_world::nbt::JavaString(vec![0xd800]))).unwrap(),
        hash::utf16(&[0xd800])
    );
    assert!(prediction::nbt(&Tag::End).is_err());
}
#[test]
fn cached_predictions_restore_map_color_and_original_custom_data_hashes() {
    let custom = Tag::compound([("test:original", Tag::Int(8))]);
    let mut cache = Predictions::default();
    let mut incoming = observe(&mut cache, &old_stack(0x123456, Some(&custom)));
    incoming.count = 12;
    incoming.added.push((3, hash::int(5)));
    incoming.added.push((24, 123));
    let old = cache.convert(&incoming).unwrap();
    assert_eq!((old.item, old.count), (72, 12));
    assert_eq!(
        old.added,
        vec![
            (0, prediction::nbt(&custom).unwrap()),
            (3, hash::int(5)),
            (45, hash::int(0x123456))
        ]
    );
    assert!(old.removed.is_empty());
}
#[test]
fn translator_custom_data_and_explicit_removals_round_trip_without_leaking() {
    let mut cache = Predictions::default();
    let incoming = observe(&mut cache, &old_stack(12, None));
    assert_eq!(
        cache.convert(&incoming).unwrap().added,
        vec![(45, hash::int(12))]
    );
    for removed in [vec![45], vec![0, 45]] {
        let mut w = Writer::new();
        w.varint(1);
        w.varint(72);
        w.varint(0);
        w.varint(removed.len() as i32);
        for &id in &removed {
            w.varint(id);
        }
        assert_eq!(convert(&convert(&w.data, true), false), w.data);
        let incoming = observe(&mut cache, &w.data);
        let old = cache.convert(&incoming).unwrap();
        assert!(old.added.is_empty());
        assert_eq!(old.removed, removed);
    }
}
#[test]
fn cache_eviction_and_separate_connections_cannot_restore_another_items_fields() {
    let mut cache = Predictions::new(1);
    let first = observe(&mut cache, &old_stack(1, None));
    let second = observe(&mut cache, &old_stack(2, None));
    assert_eq!(
        cache.convert(&second).unwrap().added,
        vec![(45, hash::int(2))]
    );
    assert!(
        !cache
            .convert(&first)
            .unwrap()
            .added
            .iter()
            .any(|&(id, _)| id == 45)
    );
    assert!(
        !Predictions::default()
            .convert(&second)
            .unwrap()
            .added
            .iter()
            .any(|&(id, _)| id == 45)
    );
}
#[test]
fn malformed_predictions_fail_and_unrepresentable_hashes_request_resynchronization() {
    let cache = Predictions::default();
    let mut stack = Hashed {
        item: 73,
        count: 1,
        added: vec![(24, 9), (40, 1), (41, 2)],
        removed: vec![84],
    };
    let old = cache.convert(&stack).unwrap();
    assert!(old.added.is_empty());
    assert!(old.removed.is_empty());
    stack.added = vec![(3, 1), (3, 2)];
    assert!(cache.convert(&stack).is_err());
    stack.added = vec![(3, 1)];
    stack.removed = vec![3];
    assert!(cache.convert(&stack).is_err());
}
fn fixture_session() -> horizon_protocol::vfp_session::Session {
    let mut session = horizon_protocol::vfp_session::Session::for_protocol_fixture().unwrap();
    let mut success = vec![0; 16];
    core::put_string(&mut success, "Horizon");
    success.push(0);
    success.extend([0; 16]);
    session.clientbound(2, &success).unwrap();
    session.serverbound(&[3]).unwrap();
    session.clientbound(3, &[]).unwrap();
    session.poll().unwrap().unwrap();
    session.poll().unwrap().unwrap();
    session.serverbound(&[3]).unwrap();
    session
}
#[test]
fn session_tracks_inventory_packets_and_rewrites_carried_and_changed_predictions() {
    let mut session = fixture_session();
    let (id, bytes) = session
        .clientbound(
            wire::packet_id(t::OLD_CLIENTBOUND, "SET_CURSOR_ITEM").unwrap(),
            &old_stack(17, None),
        )
        .unwrap();
    let play::Clientbound::SetCursorItem(p) = play::Clientbound::decode(id, &bytes).unwrap() else {
        panic!()
    };
    let stack = p.item.unwrap();
    let Some(Component::Nbt(tag)) = stack.component(0) else {
        panic!()
    };
    let predicted = Hashed {
        item: 73,
        count: 1,
        added: vec![(0, prediction::nbt(tag).unwrap()), (24, 9)],
        removed: Vec::new(),
    };
    let encoded = play::Serverbound::ContainerClick(play::ContainerClick {
        container: 0,
        state: 1,
        slot: 2,
        button: 0,
        input: 0,
        changed: vec![(2, Some(predicted.clone()))],
        carried: Some(predicted),
    })
    .encode();
    let packets = session.serverbound(&encoded).unwrap();
    assert_eq!(packets.len(), 1);
    let mut r = Reader::new(&packets[0]);
    r.varint().unwrap();
    let play::Serverbound::ContainerClick(p) = play::Serverbound::decode(
        wire::packet_id(t::NEW_SERVERBOUND, "CONTAINER_CLICK").unwrap(),
        r.rest(),
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(p.carried.as_ref().unwrap().added, vec![(45, hash::int(17))]);
    assert_eq!(p.changed[0].1, p.carried);
}
