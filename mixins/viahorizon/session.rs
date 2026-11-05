// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Error, Result, vfp_core as core, vfp_wire::Packet};
use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicI32, Ordering},
};

// Read by rendering/input without taking the packet translator's mutex.
#[derive(Default)]
pub struct PlayCapabilities {
    protocol: AtomicI32,
}
impl PlayCapabilities {
    pub fn protocol(&self) -> Option<i32> {
        let protocol = self.protocol.load(Ordering::Acquire);
        (protocol != 0).then_some(protocol)
    }
    pub fn full_items(&self) -> bool {
        matches!(self.protocol.load(Ordering::Acquire), 1..=769)
    }
    pub fn legacy_protocol(&self) -> Option<i32> {
        let protocol = self.protocol.load(Ordering::Acquire);
        (1..=404).contains(&protocol).then_some(protocol)
    }
    pub fn unsequenced_blocks(&self) -> bool {
        matches!(self.protocol.load(Ordering::Acquire), 1..=758)
    }
}

#[cfg(test)]
mod capability_tests {
    use super::*;
    #[test]
    fn published_capabilities_follow_connection_phase_and_protocol_boundaries() {
        for version in [
            "1.7.2", "1.7.10", "1.13.2", "1.14", "1.18.2", "1.19", "1.21.4", "1.21.5", "26.2",
        ] {
            let mut session = Session::for_release_fixture(version).unwrap();
            let snapshot = session.capabilities();
            assert!(!snapshot.full_items());
            assert_eq!(snapshot.legacy_protocol(), None);
            assert!(!snapshot.unsequenced_blocks());
            session.set_phase(Phase::Play);
            assert_eq!(snapshot.full_items(), session.full_item_predictions());
            assert_eq!(snapshot.legacy_protocol(), session.legacy_play_protocol());
            assert_eq!(snapshot.unsequenced_blocks(), session.unsequenced_blocks());
            session.set_phase(Phase::Configuration);
            assert!(!snapshot.full_items());
            assert_eq!(snapshot.legacy_protocol(), None);
            assert!(!snapshot.unsequenced_blocks());
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Login,
    Configuration,
    Play,
}
struct Queued {
    packet: Packet,
    mining: bool,
}
impl Queued {
    fn network(packet: Packet) -> Self {
        Self {
            packet,
            mining: false,
        }
    }
}
pub struct Session {
    capabilities: Arc<PlayCapabilities>,
    chat_signing: Option<crate::vfp_chat_signing::Signing>,
    login: core::LoginSession,
    phase: Phase,
    pending: VecDeque<Queued>,
    boundary: bool,
    allow_gameplay: bool,
    finish_offered: bool,
    finish_received: bool,
    reconfigure_requested: bool,
    mining: crate::vfp_mining::Mining,
    predictions: crate::vfp_prediction::Predictions,
    legacy: Option<crate::vfp_transition_261::Entities>,
    encrypted: bool,
    registry: crate::vfp_registry_access::Registries,
    predecessor_configuration: Option<crate::vfp_configuration_12111::Configuration>,
    predecessor: Option<crate::vfp_transition_12111::Transition>,
    older_configuration: Option<crate::vfp_configuration_1219::Configuration>,
    older: Option<crate::vfp_transition_1219::Transition>,
    ancient_configuration: Option<crate::vfp_configuration_1217::Configuration>,
    ancient: Option<crate::vfp_transition_1217::Transition>,
    legacy_1214: Option<crate::vfp_transition_1214::Transition>,
    legacy_121: Option<crate::vfp_transition_121::Transition>,
    legacy_1203: Option<crate::vfp_transition_1203::Transition>,
    bridge: Option<crate::vfp_transition_120::Bridge>,
    legacy_1193: Option<crate::vfp_transition_1193::Transition>,
    older_release: Option<crate::vfp_release_chain::Chain>,
}
impl Session {
    pub fn capabilities(&self) -> Arc<PlayCapabilities> {
        self.capabilities.clone()
    }
    fn set_phase(&mut self, phase: Phase) {
        self.phase = phase;
        self.capabilities.protocol.store(
            if phase == Phase::Play {
                self.login.target.protocol
            } else {
                0
            },
            Ordering::Release,
        );
    }
    pub fn unsequenced_blocks(&self) -> bool {
        self.phase == Phase::Play && self.login.target.protocol < 759
    }

    pub fn legacy_play_protocol(&self) -> Option<i32> {
        (self.phase == Phase::Play && self.login.target.protocol <= 404)
            .then_some(self.login.target.protocol)
    }

    pub fn full_item_predictions(&self) -> bool {
        self.phase == Phase::Play && self.login.target.protocol < 770
    }
    pub fn preview(target: &'static core::Release) -> core::Result<Self> {
        Self::create(target, false)
    }
    pub fn gameplay(target: &'static core::Release) -> core::Result<Self> {
        if target.stage != core::Stage::Gameplay {
            return Err("No gameplay translator for this target".into());
        }
        Self::create(target, true)
    }
    fn create(target: &'static core::Release, implementation: bool) -> core::Result<Self> {
        let chat_signing = implementation
            .then(crate::vfp_chat_signing::take_prepared)
            .flatten();
        let identity = chat_signing
            .as_ref()
            .map(crate::vfp_chat_signing::Signing::identity);
        let mut login = if implementation {
            core::LoginSession::for_implementation(target)?
        } else {
            core::LoginSession::new(target)?
        };
        if matches!(target.protocol, 759..=760) {
            if let Some(identity) = &identity {
                let signature = if target.protocol == 759 {
                    identity
                        .key_signature_v1
                        .as_ref()
                        .ok_or("The account certificate has no 1.19.0 signature")?
                } else {
                    &identity.key_signature
                };
                login.set_profile_key(core::LoginProfileKey {
                    expires_at: identity.expires_at,
                    public_key: identity.public_key.clone(),
                    signature: signature.clone(),
                })?;
            }
        }
        let older_release = if target.protocol < 761 {
            Some(
                crate::vfp_release_chain::Chain::new(target.protocol, identity)
                    .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
        Ok(Self {
            capabilities: Arc::new(PlayCapabilities::default()),
            chat_signing,
            login,
            phase: Phase::Login,
            pending: VecDeque::new(),
            boundary: false,
            allow_gameplay: implementation,
            finish_offered: false,
            finish_received: false,
            reconfigure_requested: false,
            mining: crate::vfp_mining::Mining::default(),
            predictions: crate::vfp_prediction::Predictions::default(),
            legacy: if target.protocol <= 775 {
                Some(crate::vfp_transition_261::Entities::new().map_err(|e| e.to_string())?)
            } else {
                None
            },
            encrypted: false,
            registry: crate::vfp_registry_access::Registries::default(),
            predecessor_configuration: (target.protocol == 774)
                .then(crate::vfp_configuration_12111::Configuration::default),
            predecessor: (target.protocol <= 774)
                .then(crate::vfp_transition_12111::Transition::default),
            older_configuration: (target.protocol == 773)
                .then(crate::vfp_configuration_1219::Configuration::default),
            older: (target.protocol <= 773).then(crate::vfp_transition_1219::Transition::default),
            ancient_configuration: (target.protocol <= 772)
                .then(|| crate::vfp_configuration_1217::Configuration::new(target.protocol)),
            ancient: (target.protocol <= 772).then(crate::vfp_transition_1217::Transition::default),
            legacy_1214: (target.protocol <= 769)
                .then(crate::vfp_transition_1214::Transition::default),
            legacy_121: (target.protocol <= 767)
                .then(crate::vfp_transition_121::Transition::default),
            legacy_1203: (target.protocol <= 765)
                .then(crate::vfp_transition_1203::Transition::default),
            bridge: (target.protocol < 764).then(crate::vfp_transition_120::Bridge::default),
            legacy_1193: (target.protocol < 762)
                .then(crate::vfp_transition_1193::Transition::default),
            older_release,
        })
    }
    // Explicit library-only entry for protocol fixtures. Never selected from
    // native mod settings; release capability still requires a real game proof.
    pub fn for_protocol_fixture() -> core::Result<Self> {
        Self::for_release_fixture("26.2")
    }
    pub fn for_release_fixture(version: &str) -> core::Result<Self> {
        let release = core::release(version).ok_or("Unknown fixture release")?;
        if release.legacy || !(4..=776).contains(&release.protocol) {
            return Err("Protocol fixtures require a Netty Release target".into());
        }
        Self::create(release, true)
    }
    fn bodies(packets: Vec<Packet>) -> Vec<Vec<u8>> {
        packets
            .into_iter()
            .map(|(id, payload)| {
                let mut body = Vec::new();
                core::put_varint(&mut body, id);
                body.extend(payload);
                body
            })
            .collect()
    }
    pub fn serverbound(&mut self, body: &[u8]) -> core::Result<Vec<Vec<u8>>> {
        let bodies = if self.phase == Phase::Play {
            match &mut self.chat_signing {
                Some(signing) => signing.outgoing(body)?,
                None => vec![body.to_vec()],
            }
        } else {
            vec![body.to_vec()]
        };
        let mut result = Vec::new();
        for body in bodies {
            result.extend(self.serverbound_inner(&body)?);
        }
        Ok(result)
    }
    fn serverbound_inner(&mut self, body: &[u8]) -> core::Result<Vec<Vec<u8>>> {
        match self.phase {
            Phase::Login => {
                if matches!(self.login.target.protocol, 4..=776)
                    && self.login.finished()
                    && body == [3]
                {
                    self.set_phase(Phase::Configuration);
                    if let Some(bridge) = &mut self.bridge {
                        let early = bridge.take_early();
                        for (id, data) in early {
                            self.queue_legacy_configuration(id, &data)?;
                        }
                        return Ok(Vec::new());
                    }
                    return Ok(vec![body.to_vec()]);
                }
                if self.login.target.protocol < 766 && core::Cursor::new(body).varint()? == 4 {
                    return Ok(Vec::new()); // Cookies did not exist on these servers.
                }
                if core::Cursor::new(body).varint()? == 0 {
                    if let Some(older) = &mut self.older_release {
                        let mut hello = core::Cursor::new(body);
                        hello.varint()?;
                        older.username(&hello.string(16)?);
                    }
                }
                let packet = self.login.serverbound(body)?;
                if core::Cursor::new(body).varint()? == 1 {
                    self.encrypted = true;
                }
                Ok(vec![packet])
            }
            Phase::Configuration => {
                if body == [3] && self.allow_gameplay && self.finish_offered {
                    self.set_phase(Phase::Play);
                    self.finish_offered = false;
                    if let Some(bridge) = &mut self.bridge {
                        let (packets, outgoing) = bridge.finish().map_err(|e| e.to_string())?;
                        if self.login.target.protocol <= 761 {
                            self.queue_held::<crate::vfp_mapping::V1193>(packets)?;
                        } else if self.login.target.protocol == 762 {
                            self.queue_held::<crate::vfp_mapping::V1194>(packets)?;
                        } else {
                            self.queue_held::<crate::vfp_mapping::V120>(packets)?;
                        }
                        return Ok(Self::bodies(
                            self.oldest_serverbound(outgoing)
                                .map_err(|e| e.to_string())?,
                        ));
                    }
                    return if self.login.target.protocol < 766 {
                        self.old_configuration_reply(body)
                    } else {
                        Ok(vec![body.to_vec()])
                    };
                }
                if let Some(configuration) = &self.ancient_configuration {
                    let packets = configuration.serverbound(body).map_err(|e| e.to_string())?;
                    if self.login.target.protocol >= 766 {
                        return Ok(packets);
                    }
                    let mut output = Vec::new();
                    for packet in packets {
                        output.extend(self.old_configuration_reply(&packet)?);
                    }
                    return Ok(output);
                }
                crate::vfp_configuration::serverbound_262(body)
                    .map(|p| vec![p])
                    .map_err(|e| e.to_string())
            }
            Phase::Play => {
                let mut r = core::Cursor::new(body);
                let wire_id = r.varint()?;
                let id = if wire_id == crate::vfp_full_click::INTERNAL_ID {
                    crate::vfp_wire::packet_id(
                        crate::vfp_tables::NEW_SERVERBOUND,
                        "CONTAINER_CLICK",
                    )?
                } else {
                    wire_id
                };
                if crate::vfp_tables::NEW_SERVERBOUND.get(id as usize)
                    == Some(&"SET_CREATIVE_MODE_SLOT")
                {
                    let mut item = crate::buf::Reader::new(&body[r.position()..]);
                    item.i16().map_err(|e| e.to_string())?;
                    if item.varint().map_err(|e| e.to_string())? > 0 {
                        let native = item.varint().map_err(|e| e.to_string())?;
                        if !(0..crate::vfp_tables::ITEMS.mapped_size).contains(&native) {
                            return Err("Unknown native creative item".into());
                        }
                        if !crate::vfp_release_chain::native_item_supported(
                            self.login.target.protocol,
                            native,
                        ) {
                            return Ok(Vec::new());
                        }
                    }
                }
                if self.login.target.protocol < 764
                    && crate::vfp_tables::NEW_SERVERBOUND.get(id as usize) == Some(&"PING_REQUEST")
                {
                    let data = r.take(8)?.to_vec();
                    r.end()?;
                    self.pending.push_back(Queued::network((
                        crate::vfp_wire::packet_id(
                            crate::vfp_tables::NEW_CLIENTBOUND,
                            "PONG_RESPONSE",
                        )?,
                        data,
                    )));
                    return Ok(Vec::new());
                }
                if Some("CONFIGURATION_ACKNOWLEDGED")
                    == crate::vfp_tables::NEW_SERVERBOUND.get(id as usize).copied()
                {
                    r.end()?;
                    if !self.reconfigure_requested {
                        return Err("Unsolicited configuration acknowledgement".into());
                    }
                    self.set_phase(Phase::Configuration);
                    self.reconfigure_requested = false;
                    self.finish_received = false;
                    self.reset_mining();
                    self.predictions.clear();
                    self.registry.clear();
                    if let Some(configuration) = &mut self.predecessor_configuration {
                        configuration.reset();
                    }
                    if let Some(predecessor) = &mut self.predecessor {
                        predecessor.reset();
                    }
                    if let Some(configuration) = &mut self.older_configuration {
                        configuration.reset();
                    }
                    if let Some(older) = &mut self.older {
                        older.reset();
                    }
                    if let Some(configuration) = &mut self.ancient_configuration {
                        configuration.reset();
                    }
                    if let Some(ancient) = &mut self.ancient {
                        ancient.reset();
                    }
                    if let Some(legacy) = &mut self.legacy_1214 {
                        legacy.reset();
                    }
                    if let Some(legacy) = &mut self.legacy_121 {
                        legacy.reset();
                    }
                    if let Some(legacy) = &mut self.legacy_1203 {
                        legacy.reset();
                    }
                    if let Some(bridge) = &mut self.bridge {
                        let packets = bridge.take_configuration().map_err(|e| e.to_string())?;
                        for (id, data) in packets {
                            self.queue_configuration_1202(id, &data)?;
                        }
                        return Ok(Vec::new());
                    }
                    return Ok(Self::bodies(vec![(
                        crate::vfp_wire::packet_id(
                            if self.login.target.protocol == 764 {
                                crate::vfp_tables_1202::SERVERBOUND
                            } else if self.login.target.protocol == 765 {
                                crate::vfp_tables_1203::SERVERBOUND
                            } else if self.login.target.protocol == 766 {
                                crate::vfp_tables_1205::SERVERBOUND
                            } else if self.login.target.protocol == 767 {
                                crate::vfp_tables_121::SERVERBOUND
                            } else if self.login.target.protocol == 768 {
                                crate::vfp_tables_1212::SERVERBOUND
                            } else if self.login.target.protocol == 769 {
                                crate::vfp_tables_1214::SERVERBOUND
                            } else if self.login.target.protocol == 770 {
                                crate::vfp_tables_1215::SERVERBOUND
                            } else if self.predecessor.is_some() {
                                crate::vfp_tables_12111::SERVERBOUND
                            } else {
                                crate::vfp_tables::OLD_SERVERBOUND
                            },
                            "CONFIGURATION_ACKNOWLEDGED",
                        )?,
                        Vec::new(),
                    )]));
                }
                let payload = &body[r.position()..];
                let name = crate::vfp_tables::NEW_SERVERBOUND
                    .get(id as usize)
                    .copied()
                    .ok_or("Unknown play packet")?;
                let mut packets = match self.login.target.protocol {
                    4..=761 => self.send_play::<crate::vfp_mapping::V1193>(wire_id, payload, name),
                    762 => self.send_play::<crate::vfp_mapping::V1194>(wire_id, payload, name),
                    763 => self.send_play::<crate::vfp_mapping::V120>(wire_id, payload, name),
                    764 => self.send_play::<crate::vfp_mapping::V1202>(wire_id, payload, name),
                    765 => self.send_play::<crate::vfp_mapping::V1203>(wire_id, payload, name),
                    766 => self.send_play::<crate::vfp_mapping::V1205>(wire_id, payload, name),
                    767 => self.send_play::<crate::vfp_mapping::V121>(wire_id, payload, name),
                    768 => self.send_play::<crate::vfp_mapping::V1212>(wire_id, payload, name),
                    769 => self.send_play::<crate::vfp_mapping::V1214>(wire_id, payload, name),
                    770 => self.send_play::<crate::vfp_mapping::V1215>(wire_id, payload, name),
                    771 => self.send_play::<crate::vfp_mapping::V1216>(wire_id, payload, name),
                    772 => self.send_play::<crate::vfp_mapping::V1217>(wire_id, payload, name),
                    773 => self.send_play::<crate::vfp_mapping::V1219>(wire_id, payload, name),
                    774 => self.send_play::<crate::vfp_mapping::V12111>(wire_id, payload, name),
                    775 => self.send_play::<crate::vfp_mapping::V261>(wire_id, payload, name),
                    776 => self.send_play::<crate::vfp_mapping::V262>(wire_id, payload, name),
                    _ => return Err("No implemented play translator".into()),
                }
                .map_err(|e| e.to_string())?;
                if let Some(legacy) = &self.legacy {
                    legacy
                        .serverbound(&mut packets)
                        .map_err(|e| e.to_string())?;
                }
                if let Some(predecessor) = &mut self.predecessor {
                    packets = predecessor
                        .serverbound(packets)
                        .map_err(|e| e.to_string())?;
                }
                if let Some(older) = &mut self.older {
                    packets = older.serverbound(packets).map_err(|e| e.to_string())?;
                }
                if let Some(ancient) = &mut self.ancient {
                    packets = ancient
                        .serverbound(self.login.target.protocol, packets)
                        .map_err(|e| e.to_string())?;
                }
                if let Some(legacy) = &mut self.legacy_1214 {
                    packets = legacy
                        .serverbound(self.login.target.protocol, packets)
                        .map_err(|e| e.to_string())?;
                }
                if let Some(legacy) = &mut self.legacy_121 {
                    packets = legacy
                        .serverbound(self.login.target.protocol, packets)
                        .map_err(|e| e.to_string())?;
                }
                if let Some(legacy) = &mut self.legacy_1203 {
                    packets = legacy.serverbound(packets).map_err(|e| e.to_string())?;
                }
                if self.login.target.protocol < 765 {
                    packets = crate::vfp_transition_1202::serverbound(packets)
                        .map_err(|e| e.to_string())?;
                }
                if let Some(bridge) = &mut self.bridge {
                    packets = bridge
                        .serverbound(self.login.target.protocol, packets)
                        .map_err(|e| e.to_string())?;
                }
                packets = self
                    .oldest_serverbound(packets)
                    .map_err(|e| e.to_string())?;
                if let Some(event) = self.mining.serverbound(name, payload)? {
                    self.pending.push_back(Queued {
                        packet: event,
                        mining: true,
                    });
                }
                if !self.mining.ongoing()
                    || (name == "PLAYER_ACTION" && core::Cursor::new(payload).varint()? == 0)
                {
                    self.pending.retain(|v| !v.mining);
                }
                Ok(Self::bodies(packets))
            }
        }
    }
    pub fn clientbound(&mut self, id: i32, payload: &[u8]) -> core::Result<Packet> {
        self.ingest(id, payload)?
            .ok_or_else(|| "Packet was suppressed by the translator".into())
    }
    pub fn ingest(&mut self, id: i32, payload: &[u8]) -> core::Result<Option<Packet>> {
        if self.phase != Phase::Login || self.login.finished() {
            let packets = if let Some(older) = &mut self.older_release {
                older
                    .clientbound(id, payload)
                    .map_err(|error| error.to_string())?
            } else {
                vec![(id, payload.to_vec())]
            };
            for (id, data) in packets {
                self.queue_old_canonical(id, &data)?;
            }
        } else {
            self.queue_ingest(id, payload)?;
        }
        let packet = self.pending.pop_front().map(|value| value.packet);
        if self.phase == Phase::Configuration && packet.as_ref().is_some_and(|packet| packet.0 == 3)
        {
            self.finish_offered = true;
        }
        Ok(packet)
    }
    fn queue_old_canonical(&mut self, id: i32, payload: &[u8]) -> core::Result<()> {
        let packets = if let Some(legacy) = &mut self.legacy_1193 {
            legacy
                .clientbound(id, payload)
                .map_err(|error| error.to_string())?
        } else {
            vec![(id, payload.to_vec())]
        };
        for (id, data) in packets {
            self.queue_ingest(id, &data)?;
        }
        Ok(())
    }
    fn queue_ingest(&mut self, id: i32, payload: &[u8]) -> core::Result<()> {
        match self.phase {
            Phase::Login => {
                if self.login.finished() {
                    if let Some(bridge) = &mut self.bridge {
                        bridge.hold(id, payload).map_err(|e| e.to_string())?;
                        return Ok(());
                    }
                }
                let packet = self.login.clientbound(id, payload)?;
                self.pending.push_back(Queued::network((id, packet)));
                Ok(())
            }
            Phase::Configuration => {
                if self.bridge.is_some() {
                    self.queue_legacy_configuration(id, payload)?;
                    return Ok(());
                }
                let packets = if self.login.target.protocol < 765 {
                    crate::vfp_transition_1202::configuration(id, payload)
                        .map_err(|e| e.to_string())?
                } else {
                    vec![(id, payload.to_vec())]
                };
                let packets = if self.login.target.protocol < 766 {
                    let mut expanded = Vec::new();
                    for (id, data) in packets {
                        expanded.extend(
                            crate::vfp_configuration_1203::clientbound(id, &data)
                                .map_err(|e| e.to_string())?,
                        );
                    }
                    expanded
                } else {
                    packets
                };
                for (id, data) in packets {
                    self.queue_configuration(id, &data)?;
                }
                Ok(())
            }
            Phase::Play => {
                if self.bridge.is_some() {
                    if self.login.target.protocol <= 761 {
                        self.queue_120::<crate::vfp_mapping::V1193>(id, payload)?;
                    } else if self.login.target.protocol == 762 {
                        self.queue_120::<crate::vfp_mapping::V1194>(id, payload)?;
                    } else {
                        self.queue_120::<crate::vfp_mapping::V120>(id, payload)?;
                    }
                    return Ok(());
                }
                if self.login.target.protocol == 764 {
                    for (id, data) in crate::vfp_transition_1202::clientbound::<
                        crate::vfp_mapping::V1202,
                    >(id, payload)
                    .map_err(|e| e.to_string())?
                    {
                        self.queue_1203::<crate::vfp_mapping::V1202>(id, &data)?;
                    }
                    return Ok(());
                }
                if self.legacy_1203.is_some() {
                    return self.queue_1203::<crate::vfp_mapping::V1203>(id, payload);
                }
                if self.legacy_121.is_some() {
                    return if self.login.target.protocol == 766 {
                        self.queue_121::<crate::vfp_mapping::V1205>(id, payload)
                    } else {
                        self.queue_121::<crate::vfp_mapping::V121>(id, payload)
                    };
                }
                let packets = if let Some(legacy) = &mut self.legacy_1214 {
                    if self.login.target.protocol == 768 {
                        legacy.clientbound_using::<crate::vfp_mapping::V1212>(
                            id,
                            payload,
                            &self.registry,
                        )
                    } else {
                        legacy.clientbound_using::<crate::vfp_mapping::V1214>(
                            id,
                            payload,
                            &self.registry,
                        )
                    }
                    .map_err(|e| e.to_string())?
                } else {
                    vec![(id, payload.to_vec())]
                };
                match self.login.target.protocol {
                    768 => self.queue_play::<crate::vfp_mapping::V1212>(packets),
                    769 => self.queue_play::<crate::vfp_mapping::V1214>(packets),
                    770 => self.queue_play::<crate::vfp_mapping::V1215>(packets),
                    771 => self.queue_play::<crate::vfp_mapping::V1216>(packets),
                    772 => self.queue_play::<crate::vfp_mapping::V1217>(packets),
                    773 => self.queue_play::<crate::vfp_mapping::V1219>(packets),
                    774 => self.queue_play::<crate::vfp_mapping::V12111>(packets),
                    775 => self.queue_play::<crate::vfp_mapping::V261>(packets),
                    776 => self.queue_play::<crate::vfp_mapping::V262>(packets),
                    _ => Err("No implemented play translator".into()),
                }
            }
        }
    }
    fn queue_configuration(&mut self, id: i32, payload: &[u8]) -> core::Result<()> {
        if id == 3 && payload.is_empty() {
            if self.finish_received {
                return Err("Duplicate finish-configuration packet".into());
            }
            self.finish_received = true;
            if let Some(configuration) = &mut self.ancient_configuration {
                let packets = configuration
                    .finish(&mut self.registry)
                    .map_err(|e| e.to_string())?;
                self.record_registries(&packets)
                    .map_err(|e| e.to_string())?;
                self.pending
                    .extend(packets.into_iter().map(Queued::network));
            }
            if let Some(configuration) = &mut self.older_configuration {
                let packets = configuration
                    .finish(&mut self.registry)
                    .map_err(|e| e.to_string())?;
                self.record_registries(&packets)
                    .map_err(|e| e.to_string())?;
                self.pending
                    .extend(packets.into_iter().map(Queued::network));
            }
            if let Some(configuration) = &mut self.predecessor_configuration {
                let packets = configuration
                    .finish(&mut self.registry)
                    .map_err(|e| e.to_string())?;
                self.record_registries(&packets)
                    .map_err(|e| e.to_string())?;
                self.pending
                    .extend(packets.into_iter().map(Queued::network));
            }
            self.pending.extend(
                crate::vfp_configuration::missing_registries_262()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .map(Queued::network),
            );
            if self.allow_gameplay {
                self.pending.push_back(Queued::network((3, Vec::new())));
            } else {
                self.boundary = true;
            }
            return Ok(());
        }
        let mut packet = if let Some(configuration) = &mut self.ancient_configuration {
            configuration.clientbound(id, payload, &mut self.registry)
        } else if let Some(configuration) = &mut self.older_configuration {
            configuration.clientbound(id, payload, &mut self.registry)
        } else if let Some(configuration) = &mut self.predecessor_configuration {
            configuration.clientbound(id, payload, &mut self.registry)
        } else if self.login.target.protocol == 775 {
            crate::vfp_configuration_261::clientbound(id, payload)
        } else {
            crate::vfp_configuration::clientbound_262(id, payload)
        }
        .map_err(|e| e.to_string())?;
        crate::vfp_dialog::configuration(&mut packet, self.login.target.protocol)
            .map_err(|e| e.to_string())?;
        self.record_registries(std::slice::from_ref(&packet))
            .map_err(|e| e.to_string())?;
        self.pending.push_back(Queued::network(packet));
        Ok(())
    }
    fn queue_legacy_configuration(&mut self, id: i32, payload: &[u8]) -> core::Result<()> {
        let bridge = self.bridge.as_mut().expect("configuration bridge");
        let packets = if self.login.target.protocol <= 761 {
            bridge.configuration::<crate::vfp_mapping::V1193>(id, payload)
        } else if self.login.target.protocol == 762 {
            bridge.configuration::<crate::vfp_mapping::V1194>(id, payload)
        } else {
            bridge.configuration::<crate::vfp_mapping::V120>(id, payload)
        }
        .map_err(|e| e.to_string())?;
        for (id, data) in packets {
            self.queue_configuration_1202(id, &data)?;
        }
        Ok(())
    }
    fn queue_configuration_1202(&mut self, id: i32, payload: &[u8]) -> core::Result<()> {
        for (id, data) in
            crate::vfp_transition_1202::configuration(id, payload).map_err(|e| e.to_string())?
        {
            for (id, data) in
                crate::vfp_configuration_1203::clientbound(id, &data).map_err(|e| e.to_string())?
            {
                self.queue_configuration(id, &data)?;
            }
        }
        Ok(())
    }
    fn old_configuration_reply(&mut self, body: &[u8]) -> core::Result<Vec<Vec<u8>>> {
        let packets =
            crate::vfp_configuration_1203::serverbound(body).map_err(|e| e.to_string())?;
        if self.login.target.protocol >= 765 {
            return Ok(packets);
        }
        let mut output = Vec::new();
        for packet in packets {
            let packets = crate::vfp_transition_1202::configuration_reply(&packet)
                .map_err(|e| e.to_string())?;
            for packet in packets {
                if let Some(bridge) = &mut self.bridge {
                    let packets = bridge
                        .configuration_reply(&packet)
                        .map_err(|e| e.to_string())?;
                    output.extend(Self::bodies(
                        self.oldest_serverbound(packets)
                            .map_err(|e| e.to_string())?,
                    ));
                } else {
                    output.push(packet);
                }
            }
        }
        Ok(output)
    }
    fn queue_1203<M: crate::vfp_mapping::Maps>(
        &mut self,
        id: i32,
        payload: &[u8],
    ) -> core::Result<()> {
        use crate::vfp_transition_121::Prepared;
        let packets = self
            .legacy_1203
            .as_mut()
            .expect("1.20.3 translator")
            .clientbound::<M>(id, payload, &mut self.registry)
            .map_err(|e| e.to_string())?;
        for packet in packets {
            match packet {
                Prepared::Native(packet) => self.queue_native_play(packet)?,
                Prepared::Source((id, data)) => self.queue_121::<M>(id, &data)?,
            }
        }
        Ok(())
    }
    fn queue_1202<M: crate::vfp_mapping::Maps>(
        &mut self,
        id: i32,
        payload: &[u8],
    ) -> core::Result<()> {
        for (id, data) in
            crate::vfp_transition_1202::clientbound::<M>(id, payload).map_err(|e| e.to_string())?
        {
            self.queue_1203::<M>(id, &data)?;
        }
        Ok(())
    }
    fn queue_held<M: crate::vfp_mapping::Maps>(
        &mut self,
        packets: Vec<crate::vfp_transition_120::Held>,
    ) -> core::Result<()> {
        for packet in packets {
            match packet {
                crate::vfp_transition_120::Held::Ready((id, data)) => {
                    self.queue_1202::<M>(id, &data)?
                }
                crate::vfp_transition_120::Held::Original((id, data)) => {
                    self.queue_120::<M>(id, &data)?
                }
            }
        }
        Ok(())
    }
    fn oldest_serverbound(&mut self, packets: Vec<Packet>) -> Result<Vec<Packet>> {
        let packets = if let Some(legacy) = &mut self.legacy_1193 {
            legacy.serverbound(packets)?
        } else {
            packets
        };
        if let Some(older) = &mut self.older_release {
            let packets = older.serverbound(packets)?;
            let local = older.take_local()?;
            for (id, data) in local {
                self.queue_old_canonical(id, &data).map_err(Error::new)?;
            }
            Ok(packets)
        } else {
            Ok(packets)
        }
    }
    fn queue_120<M: crate::vfp_mapping::Maps>(
        &mut self,
        id: i32,
        payload: &[u8],
    ) -> core::Result<()> {
        let packets = self
            .bridge
            .as_mut()
            .expect("configuration bridge")
            .play::<M>(id, payload)
            .map_err(|e| e.to_string())?;
        if let Some(packets) = packets {
            for (id, data) in packets {
                self.queue_1202::<M>(id, &data)?;
            }
        } else {
            self.reconfigure_requested = true;
            self.reset_mining();
            self.predictions.clear();
            self.pending.push_back(Queued::network((
                crate::vfp_wire::packet_id(
                    crate::vfp_tables::NEW_CLIENTBOUND,
                    "START_CONFIGURATION",
                )?,
                Vec::new(),
            )));
        }
        Ok(())
    }
    fn queue_121<M: crate::vfp_mapping::Maps>(
        &mut self,
        id: i32,
        payload: &[u8],
    ) -> core::Result<()> {
        use crate::vfp_transition_121::Prepared;
        let packets = self
            .legacy_121
            .as_mut()
            .expect("1.21 translator")
            .clientbound::<M>(id, payload, &mut self.registry)
            .map_err(|e| e.to_string())?;
        for packet in packets {
            match packet {
                Prepared::Native(packet) => self.queue_native_play(packet)?,
                Prepared::Source((id, data)) => {
                    let packets = self
                        .legacy_1214
                        .as_mut()
                        .expect("1.21.4 translator")
                        .clientbound_using::<M>(id, &data, &self.registry)
                        .map_err(|e| e.to_string())?;
                    self.queue_play::<M>(packets)?;
                }
            }
        }
        Ok(())
    }
    fn queue_play<M: crate::vfp_mapping::Maps>(&mut self, input: Vec<Packet>) -> core::Result<()> {
        for (id, payload) in input {
            let (id, prepared) = self
                .prepare_play::<M>(id, &payload)
                .map_err(|e| e.to_string())?;
            let payload = prepared.as_slice();
            let name = crate::vfp_tables::OLD_CLIENTBOUND
                .get(id as usize)
                .copied()
                .ok_or("Unknown play packet")?;
            if name == "START_CONFIGURATION" {
                if !payload.is_empty() {
                    return Err("Trailing start-configuration bytes".into());
                }
                self.reconfigure_requested = true;
                self.reset_mining();
                self.pending.push_back(Queued::network((
                    crate::vfp_wire::packet_id(
                        crate::vfp_tables::NEW_CLIENTBOUND,
                        "START_CONFIGURATION",
                    )?,
                    Vec::new(),
                )));
                continue;
            }
            let packets = self
                .receive_play::<M>(id, payload, name)
                .map_err(|e| e.to_string())?;
            if matches!(name, "LOGIN" | "RESPAWN" | "DISCONNECT") {
                self.reset_mining();
            }
            if name == "UPDATE_TAGS" {
                let config_id = crate::vfp_wire::packet_id(
                    crate::vfp_tables::NEW_CONFIGURATION,
                    "UPDATE_TAGS",
                )?;
                for (_, data) in &packets {
                    self.record_registries(&[(config_id, data.clone())])
                        .map_err(|e| e.to_string())?;
                }
            }
            for packet in packets {
                self.queue_native_play(packet)?;
            }
        }
        Ok(())
    }
    fn queue_native_play(&mut self, packet: Packet) -> core::Result<()> {
        if let Some(signing) = &mut self.chat_signing {
            signing.observe(packet.0, &packet.1)?;
        }
        use crate::packets::play;
        let name = crate::vfp_tables::NEW_CLIENTBOUND.get(packet.0 as usize);
        let dimension = match name {
            Some(&"LOGIN") => {
                match play::Clientbound::decode(packet.0, &packet.1).map_err(|e| e.to_string())? {
                    play::Clientbound::Login(login) => Some(login.spawn.dimension_type),
                    _ => unreachable!(),
                }
            }
            Some(&"RESPAWN") => {
                match play::Clientbound::decode(packet.0, &packet.1).map_err(|e| e.to_string())? {
                    play::Clientbound::Respawn(respawn) => Some(respawn.spawn.dimension_type),
                    _ => unreachable!(),
                }
            }
            _ => None,
        };
        if let Some(dimension) = dimension {
            self.registry.select_dimension(dimension)?;
        }
        self.pending.push_back(Queued::network(packet));
        Ok(())
    }
    // Every structural stage keeps static registry ids in the source domain.
    // The final typed codecs apply M exactly once, even across several stages.
    fn prepare_play<M: crate::vfp_mapping::Maps>(
        &mut self,
        id: i32,
        payload: &[u8],
    ) -> Result<Packet> {
        let mut packet = (id, payload.to_vec());
        if let Some(ancient) = &mut self.ancient {
            packet = ancient.clientbound::<M>(packet.0, &packet.1, &self.registry)?;
        }
        if let Some(older) = &mut self.older {
            packet = older.clientbound_using::<M>(packet.0, &packet.1, &self.registry)?;
        }
        if let Some(predecessor) = &mut self.predecessor {
            packet = predecessor.clientbound_using::<M>(packet.0, &packet.1, &self.registry)?;
        }
        Ok(packet)
    }
    fn send_play<M: crate::vfp_mapping::Maps>(
        &self,
        id: i32,
        payload: &[u8],
        name: &str,
    ) -> Result<Vec<Packet>> {
        if id == crate::vfp_full_click::INTERNAL_ID {
            return crate::vfp_full_click::serverbound::<M>(payload, &self.registry);
        }
        if M::PROTOCOL < 768 && matches!(name, "BUNDLE_ITEM_SELECTED" | "CLIENT_TICK_END") {
            return Ok(Vec::new());
        }
        if M::PROTOCOL < 770 && matches!(name, "TEST_INSTANCE_BLOCK_ACTION" | "SET_TEST_BLOCK") {
            return Ok(Vec::new());
        }
        if M::PROTOCOL < 769
            && matches!(
                name,
                "PLAYER_LOADED" | "PICK_ITEM_FROM_BLOCK" | "PICK_ITEM_FROM_ENTITY"
            )
        {
            return Ok(Vec::new());
        }
        if name == "CONTAINER_CLICK" {
            self.predictions.click_with::<M>(id, payload)
        } else {
            crate::vfp_items::serverbound_using::<M>(id, payload, &self.registry)
        }
    }
    fn receive_play<M: crate::vfp_mapping::Maps>(
        &mut self,
        id: i32,
        payload: &[u8],
        name: &str,
    ) -> Result<Vec<Packet>> {
        let input = if self.legacy.is_some() {
            crate::vfp_transition_261::prepare(name, payload, self.encrypted)?
        } else {
            std::borrow::Cow::Borrowed(payload)
        };
        let mut packets = crate::vfp_items::clientbound_using::<M>(id, &input, &self.registry)?;
        if let Some(legacy) = &mut self.legacy {
            legacy.clientbound(&mut packets)?;
        }
        self.predictions
            .observe_using::<M>(&packets, &self.registry)?;
        Ok(packets)
    }
    pub fn poll(&mut self) -> Option<Result<Packet>> {
        if let Some(packet) = self.pending.pop_front() {
            let packet = packet.packet;
            if self.phase == Phase::Configuration && packet.0 == 3 {
                self.finish_offered = true;
            }
            return Some(Ok(packet));
        }
        if self.boundary {
            return Some(Err(Error::new(format!(
                "ViaHorizon: {} configuration preview reached the world-entry boundary. Gameplay conversion is incomplete; world entry is disabled.",
                self.login.target.first
            ))));
        }
        None
    }
    fn record_registries(&mut self, packets: &[Packet]) -> Result<()> {
        use crate::packets::configuration as cfg;
        for (id, payload) in packets {
            let name = usize::try_from(*id)
                .ok()
                .and_then(|i| crate::vfp_tables::NEW_CONFIGURATION.get(i));
            if !matches!(name, Some(&"REGISTRY_DATA" | &"UPDATE_TAGS")) {
                continue;
            }
            match cfg::Clientbound::decode(*id, payload)? {
                cfg::Clientbound::RegistryData(data) => {
                    if data
                        .registry
                        .strip_prefix("minecraft:")
                        .unwrap_or(&data.registry)
                        == "dimension_type"
                    {
                        let heights = data
                            .entries
                            .iter()
                            .map(|entry| {
                                match entry.data.as_ref().and_then(|tag| tag.get("height")) {
                                    Some(horizon_world::nbt::Tag::Int(height)) => Some(*height),
                                    _ => None,
                                }
                            })
                            .collect();
                        self.registry
                            .dimension_heights(heights)
                            .map_err(Error::new)?;
                    }
                    self.registry
                        .replace(&data.registry, data.entries.into_iter().map(|e| e.id))
                        .map_err(Error::new)?;
                }
                cfg::Clientbound::UpdateTags(data) => {
                    for tags in data.registries {
                        self.registry
                            .replace_tags(
                                &tags.registry,
                                tags.tags.into_iter().map(|tag| (tag.name, tag.ids)),
                            )
                            .map_err(Error::new)?;
                    }
                }
                _ => (),
            }
        }
        Ok(())
    }
    pub fn needs_event_poll(&self) -> bool {
        self.phase == Phase::Play
    }
    fn reset_mining(&mut self) {
        self.mining.reset();
        self.pending.retain(|v| !v.mining);
    }
}
