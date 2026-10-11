use adb_remote_core::{DeviceState, ErrorCode};
use adb_remote_protocol::{
    AdbProxyFrame, AgentServerMessage, DeviceInfo, DeviceRegistration, LeaseInfo,
};
use chrono::Utc;
use std::{
    collections::HashMap,
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

pub type AgentTx = mpsc::Sender<Result<AgentServerMessage, tonic::Status>>;
pub type UserStreamTx = mpsc::Sender<Result<AdbProxyFrame, tonic::Status>>;

#[derive(Debug, Clone)]
pub struct OnlineDevice {
    pub display_name: String,
    pub adb_serial: String,
    pub password_required: bool,
    pub agent_id: String,
    pub group: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AgentSession {
    pub tx: AgentTx,
    pub last_seen: Instant,
}

#[derive(Debug, Clone)]
pub struct Lease {
    pub lease_id: String,
    pub display_name: String,
    pub agent_id: String,
    pub client_hostname: String,
    pub source: String,
    pub started_at_unix_millis: i64,
    pub bytes_from_user: u64,
    pub bytes_from_device: u64,
}

#[derive(Debug)]
pub struct PendingPassword {
    pub tx: oneshot::Sender<bool>,
}

#[derive(Debug, Clone)]
pub struct RateBucket {
    pub count: u32,
    pub reset_at: Instant,
}

#[derive(Debug, Default)]
pub struct ServerState {
    pub agents: HashMap<String, AgentSession>,
    pub devices: HashMap<String, OnlineDevice>,
    pub leases_by_id: HashMap<String, Lease>,
    pub lease_by_display_name: HashMap<String, String>,
    pub user_streams: HashMap<(String, u64), UserStreamTx>,
    pub pending_passwords: HashMap<String, PendingPassword>,
    pub rate_limits: HashMap<(String, String), RateBucket>,
}

impl ServerState {
    pub fn register_agent(
        &mut self,
        agent_id: String,
        _hostname: String,
        tx: AgentTx,
        devices: Vec<DeviceRegistration>,
    ) -> Result<Vec<OnlineDevice>, ErrorCode> {
        self.remove_agent_devices(&agent_id);
        self.agents.insert(
            agent_id.clone(),
            AgentSession {
                tx,
                last_seen: Instant::now(),
            },
        );

        let mut accepted = Vec::new();
        for device in devices {
            if self.devices.contains_key(&device.display_name) {
                return Err(ErrorCode::NameAlreadyExists);
            }
            let online = OnlineDevice {
                display_name: device.display_name.clone(),
                adb_serial: device.adb_serial,
                password_required: device.password_required,
                agent_id: agent_id.clone(),
                group: device.group,
                tags: device.tags,
            };
            self.devices
                .insert(online.display_name.clone(), online.clone());
            accepted.push(online);
        }
        Ok(accepted)
    }

    pub fn touch_agent(&mut self, agent_id: &str) {
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.last_seen = Instant::now();
        }
    }

    pub fn remove_agent_devices(&mut self, agent_id: &str) -> Vec<Lease> {
        let display_names = self
            .devices
            .values()
            .filter(|d| d.agent_id == agent_id)
            .map(|d| d.display_name.clone())
            .collect::<Vec<_>>();
        for display_name in &display_names {
            self.devices.remove(display_name);
        }

        let lease_ids = self
            .leases_by_id
            .values()
            .filter(|l| l.agent_id == agent_id)
            .map(|l| l.lease_id.clone())
            .collect::<Vec<_>>();
        lease_ids
            .into_iter()
            .filter_map(|lease_id| self.release_by_id(&lease_id))
            .collect()
    }

    pub fn remove_agent(&mut self, agent_id: &str) -> Vec<Lease> {
        self.agents.remove(agent_id);
        self.remove_agent_devices(agent_id)
    }

    pub fn list_devices(&self) -> Vec<DeviceInfo> {
        let mut devices = self
            .devices
            .values()
            .map(|device| {
                let lease_id = self.lease_by_display_name.get(&device.display_name);
                let leased_by = lease_id
                    .and_then(|id| self.leases_by_id.get(id))
                    .map(|lease| lease.client_hostname.clone())
                    .unwrap_or_default();
                DeviceInfo {
                    display_name: device.display_name.clone(),
                    adb_serial: device.adb_serial.clone(),
                    password_required: device.password_required,
                    state: if lease_id.is_some() {
                        DeviceState::Leased.as_str()
                    } else {
                        DeviceState::Available.as_str()
                    }
                    .to_string(),
                    leased_by,
                    group: device.group.clone(),
                    tags: device.tags.clone(),
                }
            })
            .collect::<Vec<_>>();
        devices.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        devices
    }

    pub fn acquire_lease(
        &mut self,
        display_name: &str,
        client_hostname: String,
        source: Option<SocketAddr>,
    ) -> Result<Lease, ErrorCode> {
        let device = self
            .devices
            .get(display_name)
            .ok_or(ErrorCode::DeviceOffline)?;
        if self.lease_by_display_name.contains_key(display_name) {
            return Err(ErrorCode::DeviceBusy);
        }
        let lease = Lease {
            lease_id: Uuid::new_v4().to_string(),
            display_name: display_name.to_string(),
            agent_id: device.agent_id.clone(),
            client_hostname,
            source: source.map(|s| s.to_string()).unwrap_or_default(),
            started_at_unix_millis: Utc::now().timestamp_millis(),
            bytes_from_user: 0,
            bytes_from_device: 0,
        };
        self.lease_by_display_name
            .insert(display_name.to_string(), lease.lease_id.clone());
        self.leases_by_id
            .insert(lease.lease_id.clone(), lease.clone());
        Ok(lease)
    }

    pub fn release_by_id(&mut self, lease_id: &str) -> Option<Lease> {
        let lease = self.leases_by_id.remove(lease_id)?;
        self.lease_by_display_name.remove(&lease.display_name);
        self.user_streams
            .retain(|(route_lease_id, _), _| route_lease_id != lease_id);
        Some(lease)
    }

    pub fn release_by_display_name(&mut self, display_name: &str) -> Option<Lease> {
        let lease_id = self.lease_by_display_name.get(display_name)?.clone();
        self.release_by_id(&lease_id)
    }

    pub fn list_leases(&self) -> Vec<LeaseInfo> {
        let mut leases = self
            .leases_by_id
            .values()
            .map(|lease| LeaseInfo {
                lease_id: lease.lease_id.clone(),
                display_name: lease.display_name.clone(),
                client_hostname: lease.client_hostname.clone(),
                source: lease.source.clone(),
                started_at_unix_millis: lease.started_at_unix_millis,
                bytes_from_user: lease.bytes_from_user,
                bytes_from_device: lease.bytes_from_device,
            })
            .collect::<Vec<_>>();
        leases.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        leases
    }

    pub fn agent_tx_for_lease(&self, lease_id: &str) -> Result<AgentTx, ErrorCode> {
        let lease = self
            .leases_by_id
            .get(lease_id)
            .ok_or(ErrorCode::LeaseExpired)?;
        self.agents
            .get(&lease.agent_id)
            .map(|agent| agent.tx.clone())
            .ok_or(ErrorCode::DeviceOffline)
    }

    pub fn check_rate_limit(&mut self, source: &str, display_name: &str) -> Result<(), ErrorCode> {
        let key = (source.to_string(), display_name.to_string());
        let now = Instant::now();
        let bucket = self.rate_limits.entry(key).or_insert(RateBucket {
            count: 0,
            reset_at: now + Duration::from_secs(60),
        });
        if now > bucket.reset_at {
            bucket.count = 0;
            bucket.reset_at = now + Duration::from_secs(60);
        }
        if bucket.count >= 5 {
            return Err(ErrorCode::RateLimited);
        }
        Ok(())
    }

    pub fn record_password_failure(&mut self, source: &str, display_name: &str) {
        let key = (source.to_string(), display_name.to_string());
        let now = Instant::now();
        let bucket = self.rate_limits.entry(key).or_insert(RateBucket {
            count: 0,
            reset_at: now + Duration::from_secs(60),
        });
        if now > bucket.reset_at {
            bucket.count = 0;
            bucket.reset_at = now + Duration::from_secs(60);
        }
        bucket.count += 1;
    }

    pub fn add_user_stream(&mut self, lease_id: String, stream_id: u64, tx: UserStreamTx) {
        self.user_streams.insert((lease_id, stream_id), tx);
    }

    pub fn remove_user_stream(&mut self, lease_id: &str, stream_id: u64) {
        self.user_streams.remove(&(lease_id.to_string(), stream_id));
    }

    pub fn user_stream(&self, lease_id: &str, stream_id: u64) -> Option<UserStreamTx> {
        self.user_streams
            .get(&(lease_id.to_string(), stream_id))
            .cloned()
    }

    pub fn add_bytes_from_user(&mut self, lease_id: &str, bytes: u64) {
        if let Some(lease) = self.leases_by_id.get_mut(lease_id) {
            lease.bytes_from_user += bytes;
        }
    }

    pub fn add_bytes_from_device(&mut self, lease_id: &str, bytes: u64) {
        if let Some(lease) = self.leases_by_id.get_mut(lease_id) {
            lease.bytes_from_device += bytes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(name: &str) -> DeviceRegistration {
        DeviceRegistration {
            display_name: name.to_string(),
            adb_serial: "serial".to_string(),
            password_required: false,
            tags: vec![],
            group: String::new(),
        }
    }

    #[test]
    fn rejects_active_display_name_conflict() {
        let mut state = ServerState::default();
        let (tx, _rx) = mpsc::channel(1);
        state
            .register_agent("a".into(), "h".into(), tx.clone(), vec![reg("pixel")])
            .unwrap();
        let err = state
            .register_agent("b".into(), "h2".into(), tx, vec![reg("pixel")])
            .unwrap_err();
        assert_eq!(err, ErrorCode::NameAlreadyExists);
    }

    #[test]
    fn exclusive_lease_rejects_busy_device() {
        let mut state = ServerState::default();
        let (tx, _rx) = mpsc::channel(1);
        state
            .register_agent("a".into(), "h".into(), tx, vec![reg("pixel")])
            .unwrap();
        state.acquire_lease("pixel", "u1".into(), None).unwrap();
        let err = state.acquire_lease("pixel", "u2".into(), None).unwrap_err();
        assert_eq!(err, ErrorCode::DeviceBusy);
    }

    #[test]
    fn release_clears_busy_state() {
        let mut state = ServerState::default();
        let (tx, _rx) = mpsc::channel(1);
        state
            .register_agent("a".into(), "h".into(), tx, vec![reg("pixel")])
            .unwrap();
        let lease = state.acquire_lease("pixel", "u1".into(), None).unwrap();
        state.release_by_id(&lease.lease_id).unwrap();
        state.acquire_lease("pixel", "u2".into(), None).unwrap();
    }

    #[test]
    fn rate_limit_blocks_repeated_failures() {
        let mut state = ServerState::default();
        for _ in 0..5 {
            state.check_rate_limit("source", "pixel").unwrap();
            state.record_password_failure("source", "pixel");
        }
        let err = state.check_rate_limit("source", "pixel").unwrap_err();
        assert_eq!(err, ErrorCode::RateLimited);
    }

    #[test]
    fn release_removes_stream_routes() {
        let mut state = ServerState::default();
        let (agent_tx, _agent_rx) = mpsc::channel(1);
        state
            .register_agent("a".into(), "h".into(), agent_tx, vec![reg("pixel")])
            .unwrap();
        let lease = state.acquire_lease("pixel", "u1".into(), None).unwrap();
        let (user_tx, _user_rx) = mpsc::channel(1);
        state.add_user_stream(lease.lease_id.clone(), 42, user_tx);
        assert!(state.user_stream(&lease.lease_id, 42).is_some());
        state.release_by_id(&lease.lease_id).unwrap();
        assert!(state.user_stream(&lease.lease_id, 42).is_none());
    }
}
