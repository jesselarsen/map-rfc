//! `MapNode` — a libp2p node exposing a blocking API over a background tokio
//! swarm (TASK-012 FR-3/FR-5). One MAP session per `/map/1.0.0` substream; a
//! Kademlia DHT replaces `MeshIndex` behind the same publish/lookup shape.
//!
//! Async is confined here: `MapNode`'s methods block, so no other crate changes
//! its programming model. The swarm runs in a task; commands cross via channels.

use crate::transport::Libp2pTransport;
use crate::NetError;
use futures::StreamExt;
use libp2p::kad::{self, store::MemoryStore};
use libp2p::core::transport::ListenerId;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{identify, identity, Multiaddr, PeerId, StreamProtocol, Swarm};
use map_discovery::{sharded_capability_key, Descriptor};
use map_identity::{AgentId, Keypair};
use map_core::Hello;
use map_session::{Session, SessionConfig, SystemClock, RandomMsgIds};
use std::collections::HashMap;
use std::sync::mpsc as std_mpsc;
use tokio::sync::{mpsc, oneshot};

/// The MAP application-layer protocol carried over libp2p substreams.
pub const MAP_PROTOCOL: StreamProtocol = StreamProtocol::new("/map/1.0.0");

/// An established inbound session plus the peer's accountable identity.
pub type IncomingSession = Session<Libp2pTransport, SystemClock, RandomMsgIds>;

#[derive(NetworkBehaviour)]
struct MapBehaviour {
    stream: libp2p_stream::Behaviour,
    kad: kad::Behaviour<MemoryStore>,
    identify: identify::Behaviour,
}

/// Recover an [`AgentId`] from a libp2p `PeerId`. Works because ed25519 keys are
/// small enough to be identity-hashed (multihash code 0), so the PeerId embeds
/// the protobuf-encoded public key (identity §3.2 — PeerID ≡ AgentID).
pub(crate) fn agent_id_from_peer(peer: &PeerId) -> Option<AgentId> {
    let mh = peer.as_ref();
    if mh.code() != 0 {
        return None; // not identity-hashed — key not embedded
    }
    let pk = identity::PublicKey::try_decode_protobuf(mh.digest()).ok()?;
    let ed = pk.try_into_ed25519().ok()?;
    let mut aid = Vec::with_capacity(34);
    aid.extend_from_slice(&[0xED, 0x01]);
    aid.extend_from_slice(&ed.to_bytes());
    AgentId::from_bytes(&aid).ok()
}

/// Kademlia record key for a descriptor (discovery §5): `H("map:desc:" || cid)`.
fn desc_record_key(cid: &[u8]) -> kad::RecordKey {
    let mut k = Vec::with_capacity(9 + cid.len());
    k.extend_from_slice(b"map:desc:");
    k.extend_from_slice(cid);
    kad::RecordKey::new(&map_discovery::multihash_sha256(&k))
}

enum Command {
    Listen {
        addr: Multiaddr,
        reply: oneshot::Sender<Result<Multiaddr, String>>,
    },
    Dial {
        addr: Multiaddr,
        expected: AgentId,
        reply: oneshot::Sender<Result<Libp2pTransport, String>>,
    },
    PutRecord {
        key: kad::RecordKey,
        value: Vec<u8>,
        reply: oneshot::Sender<Result<(), String>>,
    },
    GetRecord {
        key: kad::RecordKey,
        reply: oneshot::Sender<Option<Vec<u8>>>,
    },
    StartProviding {
        key: kad::RecordKey,
        reply: oneshot::Sender<Result<(), String>>,
    },
    GetProviders {
        key: kad::RecordKey,
        reply: oneshot::Sender<Vec<AgentId>>,
    },
}

/// A libp2p node. Construct with [`MapNode::new`]; drive with the blocking
/// methods. Dropping the node stops the swarm (its command channel closes).
pub struct MapNode {
    cmd: mpsc::UnboundedSender<Command>,
    incoming: std_mpsc::Receiver<IncomingSession>,
    local: AgentId,
    hello: Hello,
    seed: [u8; 32],
    _runtime: tokio::runtime::Runtime,
}

impl MapNode {
    /// Build a node whose libp2p identity IS the MAP keypair (both derived from
    /// `seed`). Asserts PeerID ≡ AgentID (FR-3) and refuses to run otherwise.
    /// `hello` is the local negotiation offer used for every session.
    pub fn new(seed: [u8; 32], hello: Hello) -> Result<MapNode, NetError> {
        let keypair = Keypair::from_seed(seed);
        let local = keypair.agent_id();

        let mut seed_copy = seed;
        let id_keys = identity::Keypair::ed25519_from_bytes(&mut seed_copy)
            .map_err(|e| NetError::Identity(e.to_string()))?;
        let local_peer = id_keys.public().to_peer_id();

        // FR-3: the PeerID must recover exactly this AgentID.
        match agent_id_from_peer(&local_peer) {
            Some(a) if a == local => {}
            _ => return Err(NetError::Identity("PeerID does not equal AgentID".into())),
        }

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| NetError::Runtime(e.to_string()))?;

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();
        let (inc_tx, inc_rx) = std_mpsc::channel::<IncomingSession>();

        // Build the swarm inside the runtime, then spawn its event loop.
        let cfg_hello = hello.clone();
        let cfg_keypair_seed = seed;
        let build_local = local.clone();
        let loop_local = local.clone();
        let started = runtime.block_on(async move {
            let swarm = build_swarm(id_keys, local_peer)?;
            let mut control = swarm.behaviour().stream.new_control();
            let mut incoming_streams = control
                .accept(MAP_PROTOCOL)
                .map_err(|e| NetError::Swarm(format!("accept /map/1.0.0: {e}")))?;

            // Inbound acceptor: each stream → a blocking Session::respond.
            let acc_hello = cfg_hello.clone();
            let acc_inc = inc_tx.clone();
            tokio::spawn(async move {
                while let Some((peer, stream)) = incoming_streams.next().await {
                    let remote = match agent_id_from_peer(&peer) {
                        Some(a) => a,
                        None => continue, // non-recoverable peer id — drop
                    };
                    // Responder: exporter = initiator(remote) || responder(local).
                    let mut exporter = Vec::with_capacity(68);
                    exporter.extend_from_slice(remote.as_bytes());
                    exporter.extend_from_slice(build_local.as_bytes());
                    let transport = Libp2pTransport::spawn(stream, exporter);
                    let cfg = SessionConfig::new(
                        Keypair::from_seed(cfg_keypair_seed),
                        remote,
                        acc_hello.clone(),
                    );
                    let inc = acc_inc.clone();
                    // respond() blocks (transport.recv) → run off the runtime.
                    tokio::task::spawn_blocking(move || {
                        if let Ok(session) =
                            Session::respond(cfg, transport, SystemClock, RandomMsgIds)
                        {
                            let _ = inc.send(session);
                        }
                    });
                }
            });

            tokio::spawn(swarm_loop(swarm, control, cmd_rx, loop_local));
            Ok::<(), NetError>(())
        });
        started?;

        Ok(MapNode {
            cmd: cmd_tx,
            incoming: inc_rx,
            local,
            hello,
            seed,
            _runtime: runtime,
        })
    }

    pub fn local_agent(&self) -> &AgentId {
        &self.local
    }

    /// Start listening; blocks until the first concrete bound address is known
    /// and returns it (e.g. an OS-assigned `/ip4/127.0.0.1/tcp/PORT`).
    pub fn listen(&self, addr: Multiaddr) -> Result<Multiaddr, NetError> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Listen { addr, reply: tx })?;
        rx.blocking_recv()
            .map_err(|_| NetError::Closed)?
            .map_err(NetError::Swarm)
    }

    /// Dial a peer and establish a MAP session as the initiator. `expected` is
    /// the AgentID the connection MUST authenticate as (FR-3); a mismatch is a
    /// refused dial.
    pub fn dial(
        &self,
        addr: Multiaddr,
        expected: AgentId,
    ) -> Result<Session<Libp2pTransport, SystemClock, RandomMsgIds>, NetError> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Dial { addr, expected: expected.clone(), reply: tx })?;
        let transport = rx
            .blocking_recv()
            .map_err(|_| NetError::Closed)?
            .map_err(NetError::Swarm)?;
        let cfg = SessionConfig::new(Keypair::from_seed(self.seed), expected, self.hello.clone());
        Session::initiate(cfg, transport, SystemClock, RandomMsgIds).map_err(NetError::Session)
    }

    /// Pull the next inbound session (blocks until one arrives or the node stops).
    pub fn accept(&self) -> Result<IncomingSession, NetError> {
        self.incoming.recv().map_err(|_| NetError::Closed)
    }

    /// Publish a descriptor (FR-5): provider records under each sharded
    /// capability key + the descriptor envelope as a kad record.
    pub fn publish_descriptor(&self, desc: &Descriptor, envelope: &[u8]) -> Result<(), NetError> {
        let sh = map_discovery::shard(&desc.agent);
        for cap in &desc.capabilities {
            let key = kad::RecordKey::new(&sharded_capability_key(cap, sh));
            let (tx, rx) = oneshot::channel();
            self.send(Command::StartProviding { key, reply: tx })?;
            rx.blocking_recv().map_err(|_| NetError::Closed)?.map_err(NetError::Swarm)?;
        }
        let cid = map_discovery::multihash_sha256(envelope);
        let (tx, rx) = oneshot::channel();
        self.send(Command::PutRecord {
            key: desc_record_key(&cid),
            value: envelope.to_vec(),
            reply: tx,
        })?;
        rx.blocking_recv().map_err(|_| NetError::Closed)?.map_err(NetError::Swarm)
    }

    /// Providers advertising `cap` under this node's view of the DHT (FR-5).
    pub fn find_providers(&self, cap: &str, shard: u8) -> Result<Vec<AgentId>, NetError> {
        let key = kad::RecordKey::new(&sharded_capability_key(cap, shard));
        let (tx, rx) = oneshot::channel();
        self.send(Command::GetProviders { key, reply: tx })?;
        rx.blocking_recv().map_err(|_| NetError::Closed)
    }

    /// Fetch a descriptor's signed envelope bytes by cid (FR-5). The caller MUST
    /// re-verify — kad is untrusted transport (same rule as `MeshIndex`).
    pub fn fetch_descriptor(&self, cid: &[u8]) -> Result<Option<Vec<u8>>, NetError> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::GetRecord { key: desc_record_key(cid), reply: tx })?;
        rx.blocking_recv().map_err(|_| NetError::Closed)
    }

    fn send(&self, cmd: Command) -> Result<(), NetError> {
        self.cmd.send(cmd).map_err(|_| NetError::Closed)
    }
}

fn build_swarm(id_keys: identity::Keypair, local_peer: PeerId) -> Result<Swarm<MapBehaviour>, NetError> {
    use std::time::Duration;
    let swarm = libp2p::SwarmBuilder::with_existing_identity(id_keys)
        .with_tokio()
        .with_tcp(
            Default::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .map_err(|e| NetError::Swarm(e.to_string()))?
        .with_quic()
        .with_behaviour(|key| {
            let mut kad = kad::Behaviour::new(local_peer, MemoryStore::new(local_peer));
            kad.set_mode(Some(kad::Mode::Server));
            MapBehaviour {
                stream: libp2p_stream::Behaviour::new(),
                kad,
                identify: identify::Behaviour::new(identify::Config::new(
                    "/map/1.0.0".into(),
                    key.public(),
                )),
            }
        })
        .map_err(|e| NetError::Swarm(e.to_string()))?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(120)))
        .build();
    Ok(swarm)
}

/// In-flight requests awaiting a swarm event to complete.
#[derive(Default)]
struct Pending {
    get_records: HashMap<kad::QueryId, oneshot::Sender<Option<Vec<u8>>>>,
    get_providers: HashMap<kad::QueryId, (oneshot::Sender<Vec<AgentId>>, Vec<AgentId>)>,
    puts: HashMap<kad::QueryId, oneshot::Sender<Result<(), String>>>,
    provides: HashMap<kad::QueryId, oneshot::Sender<Result<(), String>>>,
    dials: HashMap<PeerId, (AgentId, oneshot::Sender<Result<Libp2pTransport, String>>)>,
    listens: HashMap<ListenerId, oneshot::Sender<Result<Multiaddr, String>>>,
}

async fn swarm_loop(
    mut swarm: Swarm<MapBehaviour>,
    control: libp2p_stream::Control,
    mut cmd_rx: mpsc::UnboundedReceiver<Command>,
    local: AgentId,
) {
    let mut p = Pending::default();
    loop {
        tokio::select! {
            event = swarm.select_next_some() => {
                handle_event(&mut swarm, event, &control, &local, &mut p).await;
            }
            cmd = cmd_rx.recv() => match cmd {
                None => break, // node dropped
                Some(c) => handle_command(c, &mut swarm, &mut p),
            },
        }
    }
}

async fn handle_event(
    swarm: &mut Swarm<MapBehaviour>,
    event: SwarmEvent<MapBehaviourEvent>,
    control: &libp2p_stream::Control,
    local: &AgentId,
    p: &mut Pending,
) {
    match event {
        SwarmEvent::NewListenAddr { listener_id, address } => {
            if let Some(reply) = p.listens.remove(&listener_id) {
                let _ = reply.send(Ok(address));
            }
        }
        SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
            // Seed kad routing so lookups can reach this peer (FR-5).
            swarm
                .behaviour_mut()
                .kad
                .add_address(&peer_id, endpoint.get_remote_address().clone());

            if let Some((expected, reply)) = p.dials.remove(&peer_id) {
                match agent_id_from_peer(&peer_id) {
                    Some(a) if a == expected => {
                        // We dialed → we are the initiator: exporter = us || remote (FR-4).
                        let mut control = control.clone();
                        let me = local.clone();
                        let remote = expected.clone();
                        tokio::spawn(async move {
                            match control.open_stream(peer_id, MAP_PROTOCOL).await {
                                Ok(stream) => {
                                    let mut exporter = Vec::with_capacity(68);
                                    exporter.extend_from_slice(me.as_bytes());
                                    exporter.extend_from_slice(remote.as_bytes());
                                    let _ = reply
                                        .send(Ok(Libp2pTransport::spawn(stream, exporter)));
                                }
                                Err(e) => {
                                    let _ = reply.send(Err(format!("open_stream: {e}")));
                                }
                            }
                        });
                    }
                    _ => {
                        let _ = reply.send(Err("authenticated PeerID != expected".into()));
                    }
                }
            }
        }
        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
            if let Some(pid) = peer_id {
                if let Some((_, reply)) = p.dials.remove(&pid) {
                    let _ = reply.send(Err(format!("dial failed: {error}")));
                }
            }
        }
        SwarmEvent::Behaviour(MapBehaviourEvent::Kad(kad::Event::OutboundQueryProgressed {
            id,
            result,
            ..
        })) => handle_kad_result(id, result, p),
        _ => {}
    }
}

fn handle_kad_result(id: kad::QueryId, result: kad::QueryResult, p: &mut Pending) {
    match result {
        kad::QueryResult::GetRecord(res) => {
            if let Some(reply) = p.get_records.remove(&id) {
                let val = match res {
                    Ok(kad::GetRecordOk::FoundRecord(r)) => Some(r.record.value),
                    _ => None,
                };
                let _ = reply.send(val);
            }
        }
        kad::QueryResult::GetProviders(res) => {
            if let Some((_, acc)) = p.get_providers.get_mut(&id) {
                if let Ok(kad::GetProvidersOk::FoundProviders { providers, .. }) = &res {
                    for peer in providers {
                        if let Some(a) = agent_id_from_peer(peer) {
                            if !acc.contains(&a) {
                                acc.push(a);
                            }
                        }
                    }
                }
                if matches!(res, Ok(kad::GetProvidersOk::FinishedWithNoAdditionalRecord { .. })) {
                    if let Some((reply, acc)) = p.get_providers.remove(&id) {
                        let _ = reply.send(acc);
                    }
                }
            }
        }
        kad::QueryResult::PutRecord(res) => {
            if let Some(reply) = p.puts.remove(&id) {
                let _ = reply.send(res.map(|_| ()).map_err(|e| e.to_string()));
            }
        }
        kad::QueryResult::StartProviding(res) => {
            if let Some(reply) = p.provides.remove(&id) {
                let _ = reply.send(res.map(|_| ()).map_err(|e| e.to_string()));
            }
        }
        _ => {}
    }
}

fn handle_command(cmd: Command, swarm: &mut Swarm<MapBehaviour>, p: &mut Pending) {
    match cmd {
        Command::Listen { addr, reply } => match swarm.listen_on(addr) {
            Ok(id) => {
                p.listens.insert(id, reply);
            }
            Err(e) => {
                let _ = reply.send(Err(e.to_string()));
            }
        },
        Command::Dial { addr, expected, reply } => match crate::peer_id_of(&expected) {
            Some(peer) => {
                p.dials.insert(peer, (expected, reply));
                // Dial by (expected PeerId + address): libp2p's Noise handshake
                // then verifies the authenticated PeerId equals `peer` and aborts
                // with WrongPeerId otherwise — this IS the FR-3 dial enforcement.
                let opts = DialOpts::peer_id(peer).addresses(vec![addr]).build();
                if let Err(e) = swarm.dial(opts) {
                    if let Some((_, r)) = p.dials.remove(&peer) {
                        let _ = r.send(Err(format!("dial: {e}")));
                    }
                }
            }
            None => {
                let _ = reply.send(Err("expected AgentID has no PeerID".into()));
            }
        },
        Command::PutRecord { key, value, reply } => {
            let record = kad::Record::new(key, value);
            match swarm.behaviour_mut().kad.put_record(record, kad::Quorum::One) {
                Ok(id) => {
                    p.puts.insert(id, reply);
                }
                Err(e) => {
                    let _ = reply.send(Err(e.to_string()));
                }
            }
        }
        Command::GetRecord { key, reply } => {
            let id = swarm.behaviour_mut().kad.get_record(key);
            p.get_records.insert(id, reply);
        }
        Command::StartProviding { key, reply } => {
            match swarm.behaviour_mut().kad.start_providing(key) {
                Ok(id) => {
                    p.provides.insert(id, reply);
                }
                Err(e) => {
                    let _ = reply.send(Err(e.to_string()));
                }
            }
        }
        Command::GetProviders { key, reply } => {
            let id = swarm.behaviour_mut().kad.get_providers(key);
            p.get_providers.insert(id, (reply, Vec::new()));
        }
    }
}
