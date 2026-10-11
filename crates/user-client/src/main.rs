use adb_remote_core::tls::load_client_tls;
use adb_remote_protocol::{
    adb_proxy_frame, admin_service_client::AdminServiceClient,
    user_service_client::UserServiceClient, AcquireLeaseRequest, AdbProxyFrame,
    AdminListLeasesRequest, ListDevicesRequest, ReleaseLeaseRequest, StreamClose, StreamOpen,
};
use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use futures::StreamExt;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Channel, Endpoint};
use tracing::warn;

static STREAM_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Parser)]
#[command(name = "adb-remote")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Devices(Common),
    Connect {
        display_name: String,
        #[command(flatten)]
        common: Common,
        #[arg(long, default_value_t = 5555)]
        local_port: u16,
    },
    Disconnect {
        display_name: String,
        #[command(flatten)]
        common: Common,
    },
    Leases(Common),
}

#[derive(Parser, Clone)]
struct Common {
    #[arg(long)]
    server: String,
    #[arg(long)]
    ca: PathBuf,
    #[arg(long)]
    domain: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    match cli.command {
        Command::Devices(common) => devices(common).await,
        Command::Connect {
            display_name,
            common,
            local_port,
        } => connect_cmd(display_name, common, local_port).await,
        Command::Disconnect {
            display_name,
            common,
        } => disconnect(display_name, common).await,
        Command::Leases(common) => leases(common).await,
    }
}

async fn channel(common: &Common) -> Result<Channel> {
    let tls = load_client_tls(&common.ca, &common.domain)?;
    Endpoint::from_shared(common.server.clone())?
        .tls_config(tls)?
        .connect()
        .await
        .context("connect server")
}

async fn devices(common: Common) -> Result<()> {
    let mut client = UserServiceClient::new(channel(&common).await?);
    let response = client
        .list_devices(ListDevicesRequest {})
        .await?
        .into_inner();
    if response.devices.is_empty() {
        println!("No online devices.");
    }
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
    Ok(())
}

async fn connect_cmd(display_name: String, common: Common, local_port: u16) -> Result<()> {
    let channel = channel(&common).await?;
    let mut client = UserServiceClient::new(channel.clone());
    let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown".to_string());
    let mut response = client
        .acquire_lease(AcquireLeaseRequest {
            display_name: display_name.clone(),
            password: String::new(),
            client_hostname: hostname.clone(),
        })
        .await?
        .into_inner();
    if !response.granted && response.error_code == "PasswordRequired" {
        let password = rpassword::prompt_password("Device password: ")?;
        response = client
            .acquire_lease(AcquireLeaseRequest {
                display_name: display_name.clone(),
                password,
                client_hostname: hostname,
            })
            .await?
            .into_inner();
    }
    if !response.granted {
        return Err(anyhow!(
            "lease denied: {} {}",
            response.error_code,
            response.message
        ));
    }
    let lease_id = response.lease_id;
    let bind = format!("127.0.0.1:{local_port}");
    let listener = match TcpListener::bind(&bind).await {
        Ok(listener) => listener,
        Err(error) => {
            let _ = client
                .release_lease(ReleaseLeaseRequest {
                    lease_id: lease_id.clone(),
                    display_name: display_name.clone(),
                    reason: "bind_failed".into(),
                })
                .await;
            return Err(error).with_context(|| format!("bind {bind}"));
        }
    };
    println!("Leased {display_name}. Run: adb connect {bind}");
    println!("Press Ctrl+C to release.");

    loop {
        tokio::select! {
            accept = listener.accept() => {
                let (socket, _) = accept?;
                let channel = channel.clone();
                let lease_id = lease_id.clone();
                let display_name = display_name.clone();
                tokio::spawn(async move {
                    if let Err(error) = proxy_socket(channel, socket, lease_id, display_name).await {
                        warn!(%error, "proxy socket failed");
                    }
                });
            }
            _ = tokio::signal::ctrl_c() => {
                let _ = client.release_lease(ReleaseLeaseRequest {
                    lease_id,
                    display_name,
                    reason: "ctrl_c".into(),
                }).await;
                break;
            }
        }
    }
    Ok(())
}

async fn proxy_socket(
    channel: Channel,
    socket: TcpStream,
    lease_id: String,
    display_name: String,
) -> Result<()> {
    let stream_id = STREAM_ID.fetch_add(1, Ordering::Relaxed);
    let mut client = UserServiceClient::new(channel);
    let (tx, rx) = mpsc::channel(256);
    tx.send(AdbProxyFrame {
        lease_id: lease_id.clone(),
        stream_id,
        payload: Some(adb_proxy_frame::Payload::Open(StreamOpen { display_name })),
    })
    .await?;
    let response = client.proxy_adb_stream(ReceiverStream::new(rx)).await?;
    let mut inbound = response.into_inner();
    let (mut reader, mut writer) = socket.into_split();

    let read_lease_id = lease_id.clone();
    let tx_from_tcp = tx.clone();
    let reader_task = tokio::spawn(async move {
        let mut buf = vec![0_u8; 16 * 1024];
        loop {
            let n = reader.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            tx_from_tcp
                .send(AdbProxyFrame {
                    lease_id: read_lease_id.clone(),
                    stream_id,
                    payload: Some(adb_proxy_frame::Payload::Data(buf[..n].to_vec())),
                })
                .await
                .map_err(|_| anyhow!("proxy stream closed"))?;
        }
        let _ = tx_from_tcp
            .send(AdbProxyFrame {
                lease_id: read_lease_id,
                stream_id,
                payload: Some(adb_proxy_frame::Payload::Close(StreamClose {
                    reason: "local_closed".into(),
                })),
            })
            .await;
        anyhow::Ok(())
    });

    while let Some(frame) = inbound.next().await {
        let frame = frame?;
        match frame.payload {
            Some(adb_proxy_frame::Payload::Data(data)) => writer.write_all(&data).await?,
            Some(adb_proxy_frame::Payload::Close(_)) => break,
            Some(adb_proxy_frame::Payload::Error(error)) => {
                return Err(anyhow!(
                    "remote stream error {}: {}",
                    error.code,
                    error.message
                ));
            }
            _ => {}
        }
    }
    let _ = writer.shutdown().await;
    let _ = reader_task.await;
    Ok(())
}

async fn disconnect(display_name: String, common: Common) -> Result<()> {
    let mut client = UserServiceClient::new(channel(&common).await?);
    let response = client
        .release_lease(ReleaseLeaseRequest {
            lease_id: String::new(),
            display_name,
            reason: "user".into(),
        })
        .await?
        .into_inner();
    if response.released {
        println!("released");
    } else {
        println!("not released: {}", response.message);
    }
    Ok(())
}

async fn leases(common: Common) -> Result<()> {
    let mut client = AdminServiceClient::new(channel(&common).await?);
    let response = client
        .list_server_leases(AdminListLeasesRequest {})
        .await?
        .into_inner();
    if response.leases.is_empty() {
        println!("No active leases.");
    }
    for lease in response.leases {
        println!(
            "{}\t{}\t{}\t{}",
            lease.lease_id, lease.display_name, lease.client_hostname, lease.source
        );
    }
    Ok(())
}
