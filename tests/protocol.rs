use std::{io::Cursor as IoCursor, net::TcpListener, time::Duration};
use viahorizon_native::*;

fn profile(protocol: i32) -> Vec<u8> {
    let mut p = vec![0x12; 16];
    put_string(&mut p, "Horizon");
    put_varint(&mut p, 1);
    put_string(&mut p, "textures");
    put_string(&mut p, "e30=");
    p.push(0);
    if protocol == 776 {
        p.extend_from_slice(&[0x34; 16]);
    }
    p
}
#[test]
fn catalog_covers_release_scope_without_confusing_reused_protocols() {
    assert_eq!(release("1.0.0").unwrap().protocol, 22);
    assert_eq!(release("1.4.2").unwrap().protocol, 47);
    assert!(release("1.4.2").unwrap().legacy);
    assert_eq!(modern_protocol(47).unwrap().first, "1.8");
    for r in RELEASES {
        assert_eq!(release(r.first), Some(r));
        assert_eq!(release(r.last), Some(r));
    }
    assert!(release("b1.8.1").is_none());
    assert!(release("26.4").is_none());
    assert!(release("1.21.11-snapshot").is_none());
    assert!(release("1.6.3").is_none());
    assert_eq!(release("26.3").unwrap().stage, Stage::Native);
    assert_eq!(
        RELEASES.iter().filter(|r| r.stage == Stage::Native).count(),
        1
    );
}
#[test]
fn aliases_canonicalize_all_patch_versions() {
    assert_eq!(Selection::parse("1.8.9").unwrap().key(), "1.8");
    assert_eq!(Selection::parse("1.21.10").unwrap().key(), "1.21.9");
    assert_eq!(Selection::parse("26.1.2").unwrap().key(), "26.1");
    assert!(Selection::parse("Beta").is_err());
}
#[test]
fn unsupported_targets_cannot_bypass_capability_checks() {
    assert_eq!(choose(&Selection::Native, None, false).unwrap(), None);
    assert_eq!(choose(&Selection::Auto, Some(777), false).unwrap(), None);
    assert!(
        choose(&Selection::parse("1.6.4").unwrap(), None, true)
            .unwrap_err()
            .contains("not implemented")
    );
    assert_eq!(
        choose(&Selection::Auto, Some(776), false)
            .unwrap()
            .unwrap()
            .stage,
        Stage::Gameplay
    );
    assert!(choose(&Selection::Auto, Some(78), false).is_err());
    assert_eq!(
        choose(&Selection::Auto, Some(776), true)
            .unwrap()
            .unwrap()
            .protocol,
        776
    );
    assert!(choose(&Selection::Auto, Some(-1), true).is_err());
    assert!(choose(&Selection::Auto, None, true).is_err());
    assert!(choose(&Selection::Release("invalid".into()), None, true).is_err());
}
#[test]
fn gameplay_capabilities_cover_exactly_the_netty_release_range() {
    let gameplay: Vec<_> = RELEASES
        .iter()
        .filter(|r| r.stage == Stage::Gameplay)
        .collect();
    assert_eq!(gameplay.len(), 51);
    for target in RELEASES {
        if !target.legacy && target.protocol < CLIENT_PROTOCOL {
            assert_eq!(target.stage, Stage::Gameplay);
            for preview in [false, true] {
                assert_eq!(
                    choose(&Selection::Auto, Some(target.protocol), preview).unwrap(),
                    Some(target)
                );
                assert_eq!(
                    choose(&Selection::parse(target.last).unwrap(), None, preview).unwrap(),
                    Some(target)
                );
            }
            assert!(LoginSession::new(target).is_ok());
        } else if target.stage != Stage::Native {
            assert_eq!(target.stage, Stage::Planned);
            assert!(choose(&Selection::parse(target.first).unwrap(), None, false).is_err());
        }
    }
}
#[test]
fn settings_cycle_visits_every_release_once() {
    let mut target = Selection::Native;
    let mut visited = std::collections::HashSet::new();
    for _ in 0..RELEASES.len() + 2 {
        assert!(visited.insert(target.key().to_owned()));
        target = target.next();
    }
    assert_eq!(target, Selection::Native);
}
#[test]
fn endpoint_normalization_keeps_ports_and_ipv6_separate() {
    assert_eq!(server_key("EXAMPLE.org.").unwrap(), "example.org:25565");
    assert_eq!(
        server_key("example.org:25566").unwrap(),
        "example.org:25566"
    );
    assert_eq!(server_key("[::1]").unwrap(), "[::1]:25565");
    for bad in [
        "", "a:0", "a:65536", "::1", "a:abc", "a b", "[oops]", "[::1]:",
    ] {
        assert!(server_key(bad).is_err(), "{bad}");
    }
}
#[test]
fn settings_round_trip_and_per_server_precedence() {
    let mut c = Config {
        target: Selection::Auto,
        login_preview: true,
        ..Config::default()
    };
    c.servers.insert(
        server_key("EXAMPLE.org").unwrap(),
        Selection::parse("1.8.9").unwrap(),
    );
    assert_eq!(Config::parse(&c.json()).unwrap(), c);
    assert_eq!(c.selection("example.org.", 25565).key(), "1.8");
    assert_eq!(c.selection("example.org", 25566), Selection::Auto);
    assert_eq!(Config::default().target, Selection::Native);
    assert!(!Config::default().login_preview);
}
#[test]
fn malformed_configs_are_reported_instead_of_partially_applied() {
    for text in [
        "[]",
        "{}",
        r#"{"schema":2,"target":"native","login_preview":false}"#,
        r#"{"schema":1,"target":"Beta","login_preview":true}"#,
        r#"{"schema":1,"target":"native","login_preview":"false"}"#,
        r#"{"schema":1,"target":"native","login_preview":false,"servers":{"ok":"bad"}}"#,
    ] {
        assert!(Config::parse(text).is_err(), "{text}");
    }
    assert!(Config::parse(&" ".repeat(65537)).is_err());
}
#[test]
fn settings_save_can_atomically_replace_an_existing_file() {
    let dir = std::env::temp_dir().join(format!("vfp-settings-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    let mut c = Config::default();
    c.save(&path).unwrap();
    c.target = Selection::Auto;
    c.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap(), c);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&dir).unwrap();
}
#[test]
fn handshake_translation_preserves_virtual_host_and_transfer_intent() {
    let h = Handshake {
        protocol: 777,
        host: "mc.example.org".into(),
        port: 25566,
        intent: 3,
    };
    assert_eq!(Handshake::parse(&h.body()).unwrap(), h);
    let old = Handshake {
        protocol: 776,
        ..h.clone()
    };
    let decoded = Handshake::parse(&old.body()).unwrap();
    assert_eq!(decoded.host, h.host);
    assert_eq!(decoded.port, 25566);
    assert_eq!(decoded.intent, 3);
    for len in 0..h.body().len() {
        assert!(Handshake::parse(&h.body()[..len]).is_err());
    }
    let mut trailing = h.body();
    trailing.push(0);
    assert!(Handshake::parse(&trailing).is_err());
}

#[test]
fn transferred_connections_use_login_intention_only_on_pre_1205_targets() {
    for intent in [1, 2, 3] {
        for protocol in [761, 765, 766, 776, 777] {
            let original = Handshake {
                protocol: 777,
                host: "virtual.example.org".into(),
                port: 25567,
                intent,
            };
            let converted = original.clone().retarget(protocol);
            assert_eq!(converted.host, original.host);
            assert_eq!(converted.port, original.port);
            assert_eq!(converted.protocol, protocol);
            assert_eq!(
                converted.intent,
                if protocol < 766 && intent == 3 {
                    2
                } else {
                    intent
                }
            );
            assert_eq!(Handshake::parse(&converted.body()).unwrap(), converted);
        }
    }
}
#[test]
fn bounded_varints_reject_overflow_and_truncation() {
    for n in [0, 1, 127, 128, 255, 777, i32::MAX, -1, i32::MIN] {
        let mut bytes = Vec::new();
        put_varint(&mut bytes, n);
        let mut r = Cursor::new(&bytes);
        assert_eq!(r.varint().unwrap(), n);
        r.end().unwrap();
    }
    for bad in [&[0x80][..], &[0xff, 0xff, 0xff, 0xff, 0x10], &[0x80; 6]] {
        assert!(Cursor::new(bad).varint().is_err());
    }
}
#[test]
fn string_limits_use_utf16_units_and_reject_invalid_lengths() {
    let mut bytes = Vec::new();
    put_string(&mut bytes, "🌍");
    assert!(Cursor::new(&bytes).string(1).is_err());
    assert_eq!(Cursor::new(&bytes).string(2).unwrap(), "🌍");
    assert!(Cursor::new(&[1, 0xff]).string(10).is_err());
    assert!(
        Cursor::new(&[0xff, 0xff, 0xff, 0xff, 0x0f])
            .string(10)
            .is_err()
    );
}
#[test]
fn frames_round_trip_and_limit_allocation_before_reading() {
    let mut bytes = Vec::new();
    write_frame(&mut bytes, &[0, 1, 2]).unwrap();
    assert_eq!(
        read_frame(&mut IoCursor::new(bytes)).unwrap(),
        vec![0, 1, 2]
    );
    for bad in [
        vec![0],
        vec![0x80, 0x80, 0x80, 1],
        vec![3, 0],
        vec![0xff, 0xff, 0x7f],
    ] {
        assert!(read_frame(&mut IoCursor::new(bad)).is_err());
    }
    assert!(write_frame(&mut Vec::new(), &[]).is_err());
    assert!(write_frame(&mut Vec::new(), &vec![0; MAX_PACKET + 1]).is_err());
}
#[test]
fn login_261_adds_session_uuid_but_never_enters_configuration() {
    let mut s = LoginSession::new(release("26.1").unwrap()).unwrap();
    assert_eq!(s.serverbound(&[0, 1, 2]).unwrap(), vec![0, 1, 2]);
    let old = profile(775);
    let new = s.clientbound(2, &old).unwrap();
    assert_eq!(&new[..old.len()], &old);
    assert_eq!(&new[old.len()..], &[0; 16]);
    assert!(
        s.serverbound(&[3])
            .unwrap_err()
            .to_ascii_lowercase()
            .contains("world entry is disabled")
    );
    assert!(s.clientbound(2, &old).is_err());
}
#[test]
fn login_262_preserves_session_identity_and_compression_packet() {
    let mut s = LoginSession::new(release("26.2").unwrap()).unwrap();
    assert_eq!(s.clientbound(3, &[0x80, 2]).unwrap(), vec![0x80, 2]);
    let old = profile(776);
    assert_eq!(s.clientbound(2, &old).unwrap(), old);
    assert!(
        s.serverbound(&[3])
            .unwrap_err()
            .contains("login preview succeeded")
    );
}
#[test]
fn malformed_login_never_advances_state() {
    let mut s = LoginSession::new(release("26.2").unwrap()).unwrap();
    assert!(s.serverbound(&[3]).is_err());
    assert!(s.clientbound(2, &profile(775)).is_err());
    let mut p = profile(776);
    p.push(1);
    assert!(s.clientbound(2, &p).is_err());
    let good = profile(776);
    assert_eq!(s.clientbound(2, &good).unwrap(), good);
    assert!(LoginSession::new(release("1.6.4").unwrap()).is_err());
}
#[test]
fn status_requires_a_real_protocol_number() {
    assert_eq!(
        status_json(r#"{"version":{"name":"26.3","protocol":777}}"#).unwrap(),
        ("26.3".into(), 777)
    );
    for bad in [
        "{}",
        r#"{"version":{"name":"x","protocol":"777"}}"#,
        r#"{"version":{"name":"x","protocol":4294967296}}"#,
    ] {
        assert!(status_json(bad).is_err());
    }
}
#[test]
fn tcp_probe_uses_real_handshake_and_validates_pong() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let h = Handshake::parse(&read_frame(&mut socket).unwrap()).unwrap();
        assert_eq!(h.protocol, 777);
        assert_eq!(h.intent, 1);
        assert_eq!(h.port, addr.port());
        assert_eq!(read_frame(&mut socket).unwrap(), vec![0]);
        let mut response = vec![0];
        put_string(
            &mut response,
            r#"{"version":{"name":"Fixture 1.8","protocol":47}}"#,
        );
        write_frame(&mut socket, &response).unwrap();
        let ping = read_frame(&mut socket).unwrap();
        write_frame(&mut socket, &ping).unwrap();
    });
    let status = probe(&addr.to_string(), Duration::from_secs(3)).unwrap();
    server.join().unwrap();
    assert_eq!(status.protocol, 47);
    assert_eq!(status.name, "Fixture 1.8");
}
#[test]
fn tcp_probe_rejects_mismatched_pong() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        read_frame(&mut socket).unwrap();
        read_frame(&mut socket).unwrap();
        let mut response = vec![0];
        put_string(
            &mut response,
            r#"{"version":{"name":"26.3","protocol":777}}"#,
        );
        write_frame(&mut socket, &response).unwrap();
        read_frame(&mut socket).unwrap();
        write_frame(&mut socket, &[1, 0, 0, 0, 0, 0, 0, 0, 2]).unwrap();
    });
    assert!(
        probe(&addr.to_string(), Duration::from_secs(3))
            .unwrap_err()
            .contains("Mismatched")
    );
    server.join().unwrap();
}
