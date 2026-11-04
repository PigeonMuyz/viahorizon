// SPDX-License-Identifier: GPL-3.0-or-later
// Real offline vanilla transcripts are private target/ artifacts, not fixtures
// distributed with the source. This test requires VFP_REPLAY_TRACE/RELEASE.
use horizon_protocol::{
    packets::{configuration as cfg, login, play},
    vfp_core as core,
    vfp_session::Session,
    vfp_tables as tables,
};

#[test]
#[ignore = "Requires a private vanilla wire transcript and release environment variables"]
fn replay_real_vanilla_clientbound_packets() {
    let path =
        std::env::var("VFP_REPLAY_TRACE").expect("Set VFP_REPLAY_TRACE to a private wire.jsonl");
    let version = std::env::var("VFP_REPLAY_RELEASE").expect("Set VFP_REPLAY_RELEASE");
    let text = std::fs::read_to_string(path).unwrap();
    let mut session = Session::for_release_fixture(&version).unwrap();
    let mut phase = "login";
    let mut source_packets = 0;
    let mut native_packets = 0;
    let mut unknown = std::collections::BTreeSet::new();
    for line in text.lines() {
        let record: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(record.get("proxy_error").is_none(), "{record}");
        if record["direction"] != "clientbound" {
            continue;
        }
        let id = record["id"].as_i64().unwrap() as i32;
        let hex = record["payload"].as_str().unwrap();
        let data: Vec<u8> = hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        source_packets += 1;
        let mut packet = session.ingest(id, &data).unwrap_or_else(|error| {
            panic!(
                "{version}, source packet {source_packets}, phase {}, id {id:#x}: {error}",
                record["phase"]
            )
        });
        loop {
            if let Some((native_id, payload)) = packet {
                native_packets += 1;
                match phase {
                    "login" => {
                        let decoded = login::Clientbound::decode(native_id, &payload).unwrap();
                        if matches!(decoded, login::Clientbound::LoginFinished(_)) {
                            session.serverbound(&[3]).unwrap();
                            phase = "configuration";
                        }
                    }
                    "configuration" => {
                        let decoded = cfg::Clientbound::decode(native_id, &payload).unwrap();
                        if matches!(decoded, cfg::Clientbound::FinishConfiguration(_)) {
                            session.serverbound(&[3]).unwrap();
                            phase = "play";
                        }
                    }
                    "play" => {
                        let decoded = play::Clientbound::decode(native_id, &payload).unwrap_or_else(|error| {
                            panic!("{version}, source packet {source_packets} id {id:#x}, native {native_id:#x}: {error}")
                        });
                        match decoded {
                            play::Clientbound::StartConfiguration(_) => {
                                let id = horizon_protocol::vfp_wire::packet_id(
                                    tables::NEW_SERVERBOUND,
                                    "CONFIGURATION_ACKNOWLEDGED",
                                )
                                .unwrap();
                                let mut bytes = Vec::new();
                                core::put_varint(&mut bytes, id);
                                session.serverbound(&bytes).unwrap();
                                phase = "configuration";
                            }
                            play::Clientbound::Unknown(id, _) => {
                                unknown.insert(tables::NEW_CLIENTBOUND[id as usize]);
                            }
                            _ => (),
                        }
                    }
                    _ => unreachable!(),
                }
            }
            packet = session.poll().transpose().unwrap();
            if packet.is_none() {
                break;
            }
        }
    }
    assert!(source_packets > 0, "Empty transcript");
    println!(
        "{version}: {source_packets} source packets, {native_packets} native packets, final phase {phase}, unmodeled packets {unknown:?}"
    );
}
