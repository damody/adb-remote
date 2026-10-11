mod audit;
mod state;

use adb_remote_core::{tls::load_server_tls, ErrorCode};
use adb_remote_protocol::{
    adb_proxy_frame,
    admin_service_client::AdminServiceClient,
    admin_service_server::{AdminService, AdminServiceServer},
    agent_client_message, agent_server_message,
    agent_service_server::{AgentService, AgentServiceServer},
    user_service_server::{UserService, UserServiceServer},
    AcquireLeaseRequest, AcquireLeaseResponse, AdbProxyFrame, AdminListAuditEventsRequest,
    AdminListAuditEventsResponse, AdminListDevicesRequest, AdminListDevicesResponse,
    AdminListLeasesRequest, AdminListLeasesResponse, AgentClientMessage, AgentServerMessage,
    ListDevicesRequest, ListDevicesResponse, ReleaseLeaseRequest, ReleaseLeaseResponse,
};
use anyhow::Result;
use audit::AuditStore;
use clap::{Parser, Subcommand};
use futures::StreamExt;
use state::{PendingPassword, ServerState};
use std::{net::SocketAddr, path::PathBuf, pin::Pin, sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "adb-remote-server")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init {
        #[arg(long, default_value = "adb-remote-server.sqlite3")]
        db: PathBuf,
    },
    Serve {
        #[arg(long, default_value = "0.0.0.0:8443")]
        listen: SocketAddr,
        #[arg(long, default_value = "adb-remote-server.sqlite3")]
        db: PathBuf,
        #[arg(long)]
        cert: PathBuf,
        #[arg(long)]
        key: PathBuf,
    },
    Devices {
        #[arg(long)]
        server: String,
        #[arg(long)]
        ca: PathBuf,
        #[arg(long)]
        domain: String,
    },
    Leases {
        #[arg(long)]
        server: String,
        #[arg(long)]
        ca: PathBuf,
        #[arg(long)]
        domain: String,
    },
    Audit {
        #[arg(long, default_value = "adb-remote-server.sqlite3")]
        db: PathBuf,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
}

#[derive(Clone)]
struct Services {
    state: Arc<Mutex<ServerState>>,
    audit: AuditStore,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Init { db } => {
            let audit = AuditStore::open(&db)?;
            audit.init()?;
            println!("initialized {}", db.display());
        }
        Command::Serve {
            listen,
            db,
            cert,
            key,
        } => {
            let audit = AuditStore::open(&db)?;
            let services = Services {
                state: Arc::new(Mutex::new(ServerState::default())),
                audit,
            };
            let tls = load_server_tls(&cert, &key)?;
            info!(%listen, "starting adb-remote-server");
            Server::builder()
                .tls_config(tls)?
                .add_service(AgentServiceServer::new(services.clone()))
                .add_service(UserServiceServer::new(services.clone()))
                .add_service(AdminServiceServer::new(services))
                .serve(listen)
                .await?;
        }
        Command::Devices { server, ca, domain } => {
            let mut client = AdminServiceClient::new(admin_channel(server, ca, domain).await?);
            let response = client
                .list_server_devices(AdminListDevicesRequest {})
                .await?
                .into_inner();
            for device in response.devices {
                println!(
                    "{:<28} {:<10} {:<8} {}",
                    device.display_name,
                    device.state,
                    if device.password_required {
                        "password"
                    } else {
                        "open"
                    },
                    device.adb_serial
                );
            }
        }
        Command::Leases { server, ca, domain } => {
            let mut client = AdminServiceClient::new(admin_channel(server, ca, domain).await?);
            let response = client
                .list_server_leases(AdminListLeasesRequest {})
                .await?
                .into_inner();
            for lease in response.leases {
                println!(
                    "{}\t{}\t{}\t{}",
                    lease.lease_id, lease.display_name, lease.client_hostname, lease.source
                );
            }
        }
        Command::Audit { db, limit } => {
            let audit = AuditStore::open(&db)?;
            for event in audit.list(limit)? {
                println!(
                    "#{:<4} {:<20} {:<24} lease={} source={} reason={} bytes={}/{}",
                    event.id,
                    event.event_type,
                    event.display_name,
                    event.lease_id,
                    event.source,
                    event.reason,
                    event.bytes_from_user,
                    event.bytes_from_device
                );
            }
        }
    }
    Ok(())
}

async fn admin_channel(
    server: String,
    ca: PathBuf,
    domain: String,
) -> Result<tonic::transport::Channel> {
    let tls = adb_remote_core::tls::load_client_tls(&ca, &domain)?;
    tonic::transport::Endpoint::from_shared(server)?
        .tls_config(tls)?
        .connect()
        .await
        .map_err(Into::into)
}

type ResponseStream<T> = Pin<Box<dyn futures::Stream<Item = Result<T, Status>> + Send + 'static>>;

#[tonic::async_trait]
impl AgentService for Services {
    type AgentSessionStream = ResponseStream<AgentServerMessage>;

    async fn agent_session(
        &self,
        request: Request<tonic::Streaming<AgentClientMessage>>,
    ) -> Result<Response<Self::AgentSessionStream>, Status> {
        let mut inbound = request.into_inner();
        let first = inbound
            .next()
            .await
            .ok_or_else(|| Status::invalid_argument("agent hello required"))??;
        let hello = match first.payload {
            Some(agent_client_message::Payload::Hello(hello)) => hello,
            _ => {
                return Err(Status::invalid_argument(
                    "first agent message must be hello",
                ))
            }
        };
        let agent_id = if hello.agent_id.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            hello.agent_id
        };

        let (tx, rx) = mpsc::channel(256);
        let accepted = {
            let mut state = self.state.lock().await;
            state
                .register_agent(
                    agent_id.clone(),
                    hello.hostname.clone(),
                    tx.clone(),
                    hello.devices,
                )
                .map_err(Status::from)?
        };
        for device in &accepted {
            self.audit.record(
                "device_registered",
                &device.display_name,
                "",
                &agent_id,
                "",
                0,
                0,
            );
        }

        let services = self.clone();
        let agent_id_for_task = agent_id.clone();
        tokio::spawn(async move {
            while let Some(message) = inbound.next().await {
                match message {
                    Ok(message) => {
                        if let Err(error) = services
                            .handle_agent_message(&agent_id_for_task, message)
                            .await
                        {
                            warn!(%error, "agent message handling failed");
                        }
                    }
                    Err(error) => {
                        warn!(%error, "agent stream error");
                        break;
                    }
                }
            }
            let released = services.state.lock().await.remove_agent(&agent_id_for_task);
            for lease in released {
                services.audit.record(
                    "lease_released",
                    lease.display_name,
                    lease.lease_id,
                    lease.source,
                    "device_disconnect",
                    lease.bytes_from_user,
                    lease.bytes_from_device,
                );
            }
            services
                .audit
                .record("agent_offline", "", "", agent_id_for_task, "", 0, 0);
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

impl Services {
    async fn handle_agent_message(
        &self,
        agent_id: &str,
        message: AgentClientMessage,
    ) -> Result<()> {
        match message.payload {
            Some(agent_client_message::Payload::Heartbeat(_)) => {
                self.state.lock().await.touch_agent(agent_id);
            }
            Some(agent_client_message::Payload::PasswordResult(result)) => {
                let pending = self
                    .state
                    .lock()
                    .await
                    .pending_passwords
                    .remove(&result.request_id);
                if let Some(pending) = pending {
                    let _ = pending.tx.send(result.valid);
                }
            }
            Some(agent_client_message::Payload::ProxyFrame(frame)) => {
                if let Some(adb_proxy_frame::Payload::Data(data)) = &frame.payload {
                    self.state
                        .lock()
                        .await
                        .add_bytes_from_device(&frame.lease_id, data.len() as u64);
                }
                let tx = self
                    .state
                    .lock()
                    .await
                    .user_stream(&frame.lease_id, frame.stream_id);
                if let Some(tx) = tx {
                    let _ = tx.send(Ok(frame)).await;
                }
            }
            Some(agent_client_message::Payload::Hello(_)) | None => {}
        }
        Ok(())
    }

    async fn validate_password(
        &self,
        display_name: &str,
        password: String,
        source: &str,
    ) -> Result<bool, ErrorCode> {
        {
            let mut state = self.state.lock().await;
            state.check_rate_limit(source, display_name)?;
        }

        let request_id = Uuid::new_v4().to_string();
        let (result_tx, result_rx) = oneshot::channel();
        let agent_tx = {
            let mut state = self.state.lock().await;
            let agent_id = state
                .devices
                .get(display_name)
                .ok_or(ErrorCode::DeviceOffline)?
                .agent_id
                .clone();
            let agent_tx = state
                .agents
                .get(&agent_id)
                .ok_or(ErrorCode::DeviceOffline)?
                .tx
                .clone();
            state
                .pending_passwords
                .insert(request_id.clone(), PendingPassword { tx: result_tx });
            agent_tx
        };
        let message = AgentServerMessage {
            payload: Some(agent_server_message::Payload::PasswordRequest(
                adb_remote_protocol::ValidatePasswordRequest {
                    request_id,
                    display_name: display_name.to_string(),
                    password,
                },
            )),
        };
        agent_tx
            .send(Ok(message))
            .await
            .map_err(|_| ErrorCode::DeviceOffline)?;

        match tokio::time::timeout(Duration::from_secs(15), result_rx).await {
            Ok(Ok(true)) => Ok(true),
            Ok(Ok(false)) => {
                self.state
                    .lock()
                    .await
                    .record_password_failure(source, display_name);
                self.audit
                    .record("password_failed", display_name, "", source, "", 0, 0);
                Ok(false)
            }
            _ => Err(ErrorCode::DeviceOffline),
        }
    }
}

#[tonic::async_trait]
impl UserService for Services {
    type ProxyAdbStreamStream = ResponseStream<AdbProxyFrame>;

    async fn list_devices(
        &self,
        _request: Request<ListDevicesRequest>,
    ) -> Result<Response<ListDevicesResponse>, Status> {
        let devices = self.state.lock().await.list_devices();
        Ok(Response::new(ListDevicesResponse { devices }))
    }

    async fn acquire_lease(
        &self,
        request: Request<AcquireLeaseRequest>,
    ) -> Result<Response<AcquireLeaseResponse>, Status> {
        let source = request
            .remote_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_default();
        let req = request.into_inner();
        let device = {
            let state = self.state.lock().await;
            state.devices.get(&req.display_name).cloned()
        };
        let Some(device) = device else {
            return Ok(Response::new(denied(ErrorCode::DeviceOffline, false)));
        };
        if self
            .state
            .lock()
            .await
            .lease_by_display_name
            .contains_key(&req.display_name)
        {
            return Ok(Response::new(denied(
                ErrorCode::DeviceBusy,
                device.password_required,
            )));
        }
        if device.password_required {
            if req.password.is_empty() {
                return Ok(Response::new(denied(ErrorCode::PasswordRequired, true)));
            }
            match self
                .validate_password(&req.display_name, req.password, &source)
                .await
            {
                Ok(true) => {}
                Ok(false) => return Ok(Response::new(denied(ErrorCode::InvalidPassword, true))),
                Err(error) => return Ok(Response::new(denied(error, true))),
            }
        }
        let lease = match self.state.lock().await.acquire_lease(
            &req.display_name,
            req.client_hostname,
            source.parse().ok(),
        ) {
            Ok(lease) => lease,
            Err(error) => return Ok(Response::new(denied(error, device.password_required))),
        };
        self.audit.record(
            "lease_created",
            &lease.display_name,
            &lease.lease_id,
            &lease.source,
            "",
            0,
            0,
        );
        Ok(Response::new(AcquireLeaseResponse {
            granted: true,
            lease_id: lease.lease_id,
            error_code: String::new(),
            message: "lease granted".to_string(),
            password_required: device.password_required,
        }))
    }

    async fn release_lease(
        &self,
        request: Request<ReleaseLeaseRequest>,
    ) -> Result<Response<ReleaseLeaseResponse>, Status> {
        let req = request.into_inner();
        let lease = if !req.lease_id.is_empty() {
            self.state.lock().await.release_by_id(&req.lease_id)
        } else {
            self.state
                .lock()
                .await
                .release_by_display_name(&req.display_name)
        };
        if let Some(lease) = lease {
            self.audit.record(
                "lease_released",
                &lease.display_name,
                &lease.lease_id,
                &lease.source,
                if req.reason.is_empty() {
                    "user"
                } else {
                    &req.reason
                },
                lease.bytes_from_user,
                lease.bytes_from_device,
            );
            Ok(Response::new(ReleaseLeaseResponse {
                released: true,
                error_code: String::new(),
                message: "released".to_string(),
            }))
        } else {
            Ok(Response::new(ReleaseLeaseResponse {
                released: false,
                error_code: ErrorCode::NotFound.as_str().to_string(),
                message: "lease not found".to_string(),
            }))
        }
    }

    async fn proxy_adb_stream(
        &self,
        request: Request<tonic::Streaming<AdbProxyFrame>>,
    ) -> Result<Response<Self::ProxyAdbStreamStream>, Status> {
        let mut inbound = request.into_inner();
        let first = inbound
            .next()
            .await
            .ok_or_else(|| Status::invalid_argument("stream open required"))??;
        if !matches!(first.payload, Some(adb_proxy_frame::Payload::Open(_))) {
            return Err(Status::invalid_argument("first frame must be open"));
        }
        let lease_id = first.lease_id.clone();
        let stream_id = first.stream_id;
        let agent_tx = self
            .state
            .lock()
            .await
            .agent_tx_for_lease(&lease_id)
            .map_err(Status::from)?;
        let (out_tx, out_rx) = mpsc::channel(256);
        self.state
            .lock()
            .await
            .add_user_stream(lease_id.clone(), stream_id, out_tx);

        agent_tx
            .send(Ok(AgentServerMessage {
                payload: Some(agent_server_message::Payload::ProxyFrame(first)),
            }))
            .await
            .map_err(|_| Status::from(ErrorCode::DeviceOffline))?;

        let services = self.clone();
        tokio::spawn(async move {
            while let Some(frame) = inbound.next().await {
                match frame {
                    Ok(frame) => {
                        if let Some(adb_proxy_frame::Payload::Data(data)) = &frame.payload {
                            services
                                .state
                                .lock()
                                .await
                                .add_bytes_from_user(&frame.lease_id, data.len() as u64);
                        }
                        let agent_tx = services
                            .state
                            .lock()
                            .await
                            .agent_tx_for_lease(&frame.lease_id);
                        match agent_tx {
                            Ok(agent_tx) => {
                                let _ = agent_tx
                                    .send(Ok(AgentServerMessage {
                                        payload: Some(agent_server_message::Payload::ProxyFrame(
                                            frame,
                                        )),
                                    }))
                                    .await;
                            }
                            Err(_) => break,
                        }
                    }
                    Err(error) => {
                        warn!(%error, "user proxy stream failed");
                        break;
                    }
                }
            }
            services
                .state
                .lock()
                .await
                .remove_user_stream(&lease_id, stream_id);
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(out_rx))))
    }
}

fn denied(code: ErrorCode, password_required: bool) -> AcquireLeaseResponse {
    AcquireLeaseResponse {
        granted: false,
        lease_id: String::new(),
        error_code: code.as_str().to_string(),
        message: code.as_str().to_string(),
        password_required,
    }
}

#[tonic::async_trait]
impl AdminService for Services {
    async fn list_server_devices(
        &self,
        _request: Request<AdminListDevicesRequest>,
    ) -> Result<Response<AdminListDevicesResponse>, Status> {
        Ok(Response::new(AdminListDevicesResponse {
            devices: self.state.lock().await.list_devices(),
        }))
    }

    async fn list_server_leases(
        &self,
        _request: Request<AdminListLeasesRequest>,
    ) -> Result<Response<AdminListLeasesResponse>, Status> {
        Ok(Response::new(AdminListLeasesResponse {
            leases: self.state.lock().await.list_leases(),
        }))
    }

    async fn list_audit_events(
        &self,
        request: Request<AdminListAuditEventsRequest>,
    ) -> Result<Response<AdminListAuditEventsResponse>, Status> {
        let limit = request.into_inner().limit;
        let audit = self.audit.clone();
        let events = tokio::task::spawn_blocking(move || audit.list(limit))
            .await
            .map_err(|_| Status::internal("audit task failed"))?
            .map_err(|error| Status::internal(error.to_string()))?;
        Ok(Response::new(AdminListAuditEventsResponse { events }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adb_remote_protocol::{agent_server_message, DeviceRegistration, ValidatePasswordResult};
    use tempfile::NamedTempFile;

    fn test_services() -> Services {
        let db = NamedTempFile::new().unwrap();
        Services {
            state: Arc::new(Mutex::new(ServerState::default())),
            audit: AuditStore::open(db.path()).unwrap(),
        }
    }

    fn test_device(password_required: bool) -> DeviceRegistration {
        DeviceRegistration {
            display_name: "qa-pixel".into(),
            adb_serial: "serial-1".into(),
            password_required,
            tags: vec![],
            group: String::new(),
        }
    }

    async fn register_mock_agent(
        services: &Services,
        password_required: bool,
    ) -> mpsc::Receiver<Result<AgentServerMessage, Status>> {
        let (agent_tx, agent_rx) = mpsc::channel(16);
        services
            .state
            .lock()
            .await
            .register_agent(
                "agent-1".into(),
                "host".into(),
                agent_tx,
                vec![test_device(password_required)],
            )
            .unwrap();
        agent_rx
    }

    #[tokio::test]
    async fn mock_device_registration_and_listing() {
        let services = test_services();
        let _agent_rx = register_mock_agent(&services, false).await;

        let response = services
            .list_devices(Request::new(ListDevicesRequest {}))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(response.devices.len(), 1);
        assert_eq!(response.devices[0].display_name, "qa-pixel");
        assert_eq!(response.devices[0].state, "available");
    }

    #[tokio::test]
    async fn password_protected_lease_forwards_validation_to_agent() {
        let services = test_services();
        let mut agent_rx = register_mock_agent(&services, true).await;
        let acquire_services = services.clone();

        let acquire = tokio::spawn(async move {
            acquire_services
                .acquire_lease(Request::new(AcquireLeaseRequest {
                    display_name: "qa-pixel".into(),
                    password: "secret".into(),
                    client_hostname: "user-host".into(),
                }))
                .await
                .unwrap()
                .into_inner()
        });

        let request = agent_rx.recv().await.unwrap().unwrap();
        let password_request = match request.payload.unwrap() {
            agent_server_message::Payload::PasswordRequest(request) => request,
            _ => panic!("expected password request"),
        };
        assert_eq!(password_request.display_name, "qa-pixel");
        assert_eq!(password_request.password, "secret");

        services
            .handle_agent_message(
                "agent-1",
                AgentClientMessage {
                    payload: Some(agent_client_message::Payload::PasswordResult(
                        ValidatePasswordResult {
                            request_id: password_request.request_id,
                            display_name: "qa-pixel".into(),
                            valid: true,
                        },
                    )),
                },
            )
            .await
            .unwrap();

        let response = acquire.await.unwrap();
        assert!(response.granted);
        assert!(!response.lease_id.is_empty());
    }

    #[tokio::test]
    async fn invalid_password_rejects_and_busy_device_is_rejected() {
        let services = test_services();
        let mut agent_rx = register_mock_agent(&services, true).await;
        let acquire_services = services.clone();

        let acquire = tokio::spawn(async move {
            acquire_services
                .acquire_lease(Request::new(AcquireLeaseRequest {
                    display_name: "qa-pixel".into(),
                    password: "bad".into(),
                    client_hostname: "user-host".into(),
                }))
                .await
                .unwrap()
                .into_inner()
        });

        let request = agent_rx.recv().await.unwrap().unwrap();
        let password_request = match request.payload.unwrap() {
            agent_server_message::Payload::PasswordRequest(request) => request,
            _ => panic!("expected password request"),
        };
        services
            .handle_agent_message(
                "agent-1",
                AgentClientMessage {
                    payload: Some(agent_client_message::Payload::PasswordResult(
                        ValidatePasswordResult {
                            request_id: password_request.request_id,
                            display_name: "qa-pixel".into(),
                            valid: false,
                        },
                    )),
                },
            )
            .await
            .unwrap();

        let response = acquire.await.unwrap();
        assert!(!response.granted);
        assert_eq!(response.error_code, "InvalidPassword");

        let mut state = services.state.lock().await;
        state.devices.get_mut("qa-pixel").unwrap().password_required = false;
        drop(state);

        let first = services
            .acquire_lease(Request::new(AcquireLeaseRequest {
                display_name: "qa-pixel".into(),
                password: String::new(),
                client_hostname: "user-1".into(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert!(first.granted);

        let second = services
            .acquire_lease(Request::new(AcquireLeaseRequest {
                display_name: "qa-pixel".into(),
                password: String::new(),
                client_hostname: "user-2".into(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert!(!second.granted);
        assert_eq!(second.error_code, "DeviceBusy");
    }

    #[tokio::test]
    async fn release_on_disconnect_removes_lease_and_device() {
        let services = test_services();
        let _agent_rx = register_mock_agent(&services, false).await;
        let lease = services
            .state
            .lock()
            .await
            .acquire_lease("qa-pixel", "user-host".into(), None)
            .unwrap();

        let released = services.state.lock().await.remove_agent("agent-1");

        assert_eq!(released.len(), 1);
        assert_eq!(released[0].lease_id, lease.lease_id);
        let devices = services.state.lock().await.list_devices();
        assert!(devices.is_empty());
    }
}
