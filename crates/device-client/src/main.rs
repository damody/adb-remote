use adb_remote_core::{
    adb::{check_adb_server, scan_devices},
    config::{load_device_config, save_device_config, DeviceClientConfig, SharedDeviceConfig},
    net::DEFAULT_ADB_SERVER,
    tls::load_client_tls,
};
use adb_remote_protocol::{
    adb_proxy_frame, agent_client_message, agent_server_message,
    agent_service_client::AgentServiceClient, AdbProxyFrame, AgentClientMessage, AgentHello,
    DeviceRegistration, Heartbeat, StreamClose, StreamError, ValidatePasswordResult,
};
use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use futures::StreamExt;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{mpsc, Mutex},
    time::{self, Duration},
};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Channel, Endpoint};
use tracing::info;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "adb-remote-client")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Scan,
    AddDevice {
        adb_serial: String,
        #[arg(long)]
        name: String,
        #[arg(long, conflicts_with = "no_password")]
        password: bool,
        #[arg(long, conflicts_with = "password")]
        no_password: bool,
    },
    Serve {
        #[arg(long)]
        server: String,
        #[arg(long)]
        ca: PathBuf,
        #[arg(long)]
        domain: String,
    },
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    match cli.command {
        Command::Scan => scan().await,
        Command::AddDevice {
            adb_serial,
            name,
            password,
            no_password,
        } => add_device(adb_serial, name, password, no_password),
        Command::Serve { server, ca, domain } => serve(server, ca, domain).await,
        Command::Status => status().await,
    }
}

async fn scan() -> Result<()> {
    let config = load_device_config()?;
    let devices = scan_devices(config.adb_path.as_deref())?;
    if devices.is_empty() {
        println!("No ADB devices found.");
    }
    for device in devices {
        println!("{}\t{}\t{}", device.serial, device.state, device.details);
    }
    Ok(())
}

fn add_device(
    adb_serial: String,
    display_name: String,
    password: bool,
    no_password: bool,
) -> Result<()> {
    if !password && !no_password {
        return Err(anyhow!("choose either --password or --no-password"));
    }
    let mut config = load_device_config()?;
    let password_value = if password {
        let first = rpassword::prompt_password("Device password: ")?;
        let second = rpassword::prompt_password("Confirm password: ")?;
        if first != second {
            return Err(anyhow!("passwords do not match"));
        }
        Some(first)
    } else {
        None
    };
    config.devices.retain(|d| d.adb_serial != adb_serial);
    config.devices.push(SharedDeviceConfig {
        adb_serial,
        display_name,
        password: password_value,
        group: None,
        tags: vec![],
    });
    if config.agent_id.is_none() {
        config.agent_id = Some(Uuid::new_v4().to_string());
    }
    save_device_config(&config)?;
    println!("device saved");
    Ok(())
}

async fn status() -> Result<()> {
    let config = load_device_config()?;
    println!("Config devices: {}", config.devices.len());
    println!(
        "Config path: {}",
        adb_remote_core::config::device_config_path()?.display()
    );
    match check_adb_server().await {
        Ok(()) => println!("Local ADB server: available at {DEFAULT_ADB_SERVER}"),
        Err(error) => println!("Local ADB server: unavailable ({error})"),
    }
    for device in config.devices {
        println!(
            "{} -> {} ({})",
            device.adb_serial,
            device.display_name,
            if device.password_required() {
                "password"
            } else {
                "open"
            }
        );
    }
    Ok(())
}

async fn serve(server: String, ca: PathBuf, domain: String) -> Result<()> {
    let config = load_device_config()?;
    let channel = connect(server, ca, domain).await?;
    let mut client = AgentServiceClient::new(channel);
    let (tx, rx) = mpsc::channel(256);

    let hello = build_hello(&config)?;
    tx.send(AgentClientMessage {
        payload: Some(agent_client_message::Payload::Hello(hello)),
    })
    .await?;

    let config = Arc::new(config);
    let stream_writers: Arc<Mutex<HashMap<u64, mpsc::Sender<Vec<u8>>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let response = client.agent_session(ReceiverStream::new(rx)).await?;
    info!("device agent connected");

    let heartbeat_tx = tx.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            if heartbeat_tx
                .send(AgentClientMessage {
                    payload: Some(agent_client_message::Payload::Heartbeat(Heartbeat {
                        unix_millis: chrono::Utc::now().timestamp_millis(),
                    })),
                })
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let mut inbound = response.into_inner();
    while let Some(message) = inbound.next().await {
        match message?.payload {
            Some(agent_server_message::Payload::PasswordRequest(req)) => {
                let valid = config
                    .devices
                    .iter()
                    .find(|d| d.display_name == req.display_name)
                    .and_then(|d| d.password.as_ref())
                    .map(|password| password == &req.password)
                    .unwrap_or(false);
                tx.send(AgentClientMessage {
                    payload: Some(agent_client_message::Payload::PasswordResult(
                        ValidatePasswordResult {
                            request_id: req.request_id,
                            display_name: req.display_name,
                            valid,
                        },
                    )),
                })
                .await?;
            }
            Some(agent_server_message::Payload::ProxyFrame(frame)) => {
                handle_proxy_frame(frame, tx.clone(), stream_writers.clone()).await;
            }
            None => {}
        }
    }
    Ok(())
}

async fn connect(server: String, ca: PathBuf, domain: String) -> Result<Channel> {
    let tls = load_client_tls(&ca, &domain)?;
    Endpoint::from_shared(server)?
        .tls_config(tls)?
        .connect()
        .await
        .context("connect server")
}

fn build_hello(config: &DeviceClientConfig) -> Result<AgentHello> {
    let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".to_string());
    let agent_id = config
        .agent_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let scanned = scan_devices(config.adb_path.as_deref()).unwrap_or_default();
    let devices = config
        .devices
        .iter()
        .filter(|configured| {
            scanned
                .iter()
                .any(|d| d.serial == configured.adb_serial && d.state == "device")
        })
        .map(|configured| DeviceRegistration {
            display_name: configured.display_name.clone(),
            adb_serial: configured.adb_serial.clone(),
            password_required: configured.password_required(),
            tags: configured.tags.clone(),
            group: configured.group.clone().unwrap_or_default(),
        })
        .collect();
    Ok(AgentHello {
        agent_id,
        hostname,
        devices,
    })
}

async fn handle_proxy_frame(
    frame: AdbProxyFrame,
    agent_tx: mpsc::Sender<AgentClientMessage>,
    writers: Arc<Mutex<HashMap<u64, mpsc::Sender<Vec<u8>>>>>,
) {
    match frame.payload {
        Some(adb_proxy_frame::Payload::Open(_)) => {
            let lease_id = frame.lease_id.clone();
            let stream_id = frame.stream_id;
            match TcpStream::connect(DEFAULT_ADB_SERVER).await {
                Ok(stream) => {
                    let (mut reader, mut writer) = stream.into_split();
                    let (write_tx, mut write_rx) = mpsc::channel::<Vec<u8>>(256);
                    writers.lock().await.insert(stream_id, write_tx);

                    let tx_from_adb = agent_tx.clone();
                    tokio::spawn(async move {
                        let mut buf = vec![0_u8; 16 * 1024];
                        loop {
                            match reader.read(&mut buf).await {
                                Ok(0) => break,
                                Ok(n) => {
                                    let _ = tx_from_adb
                                        .send(AgentClientMessage {
                                            payload: Some(
                                                agent_client_message::Payload::ProxyFrame(
                                                    AdbProxyFrame {
                                                        lease_id: lease_id.clone(),
                                                        stream_id,
                                                        payload: Some(
                                                            adb_proxy_frame::Payload::Data(
                                                                buf[..n].to_vec(),
                                                            ),
                                                        ),
                                                    },
                                                ),
                                            ),
                                        })
                                        .await;
                                }
                                Err(error) => {
                                    let _ = tx_from_adb
                                        .send(proxy_error(
                                            &lease_id,
                                            stream_id,
                                            "RelayFailed",
                                            error.to_string(),
                                        ))
                                        .await;
                                    break;
                                }
                            }
                        }
                        let _ = tx_from_adb
                            .send(AgentClientMessage {
                                payload: Some(agent_client_message::Payload::ProxyFrame(
                                    AdbProxyFrame {
                                        lease_id,
                                        stream_id,
                                        payload: Some(adb_proxy_frame::Payload::Close(
                                            StreamClose {
                                                reason: "adb_closed".into(),
                                            },
                                        )),
                                    },
                                )),
                            })
                            .await;
                    });

                    tokio::spawn(async move {
                        while let Some(bytes) = write_rx.recv().await {
                            if writer.write_all(&bytes).await.is_err() {
                                break;
                            }
                        }
                        let _ = writer.shutdown().await;
                    });
                }
                Err(error) => {
                    let _ = agent_tx
                        .send(proxy_error(
                            &frame.lease_id,
                            stream_id,
                            "AdbUnavailable",
                            error.to_string(),
                        ))
                        .await;
                }
            }
        }
        Some(adb_proxy_frame::Payload::Data(data)) => {
            if let Some(tx) = writers.lock().await.get(&frame.stream_id).cloned() {
                let _ = tx.send(data).await;
            }
        }
        Some(adb_proxy_frame::Payload::Close(_)) | Some(adb_proxy_frame::Payload::Error(_)) => {
            writers.lock().await.remove(&frame.stream_id);
        }
        None => {}
    }
}

fn proxy_error(lease_id: &str, stream_id: u64, code: &str, message: String) -> AgentClientMessage {
    AgentClientMessage {
        payload: Some(agent_client_message::Payload::ProxyFrame(AdbProxyFrame {
            lease_id: lease_id.to_string(),
            stream_id,
            payload: Some(adb_proxy_frame::Payload::Error(StreamError {
                code: code.to_string(),
                message,
            })),
        })),
    }
}
