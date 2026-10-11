# Linux Server Deployment

`adb-remote-server` is intended to run on Linux in production. The Windows-only assumption applies to the device-side and user-side clients, not to the server.

## Build On Linux

On a Linux x64 host:

```bash
cargo build --release -p adb-remote-server
```

The server binary is written to:

```bash
target/release/adb-remote-server
```

To create a tarball:

```bash
VERSION=0.1.0 ./scripts/build-server-linux.sh
```

The archive is written to:

```bash
dist/adb-remote-server-0.1.0-linux-x64.tar.gz
```

## Build With Docker

```bash
docker build -f Dockerfile.server -t adb-remote-server:0.1.0 .
```

Run it with mounted certs and data:

```bash
docker run --rm -p 8443:8443 \
  -v /etc/adb-remote-server:/etc/adb-remote-server:ro \
  -v /var/lib/adb-remote-server:/var/lib/adb-remote-server \
  adb-remote-server:0.1.0
```

## Install As A systemd Service

Create directories and a service user:

```bash
sudo useradd --system --home /var/lib/adb-remote-server --create-home adb-remote
sudo mkdir -p /etc/adb-remote-server /var/lib/adb-remote-server
sudo chown -R adb-remote:adb-remote /var/lib/adb-remote-server
```

Install the binary:

```bash
sudo install -m 0755 target/release/adb-remote-server /usr/local/bin/adb-remote-server
```

Place TLS files:

```bash
sudo install -m 0644 server.pem /etc/adb-remote-server/server.pem
sudo install -m 0600 server.key /etc/adb-remote-server/server.key
```

Install and start the service:

```bash
sudo cp packaging/linux/adb-remote-server.service /etc/systemd/system/adb-remote-server.service
sudo systemctl daemon-reload
sudo systemctl enable --now adb-remote-server
sudo systemctl status adb-remote-server
```

## Server Command

The systemd unit runs:

```bash
adb-remote-server serve \
  --listen 0.0.0.0:8443 \
  --db /var/lib/adb-remote-server/adb-remote-server.sqlite3 \
  --cert /etc/adb-remote-server/server.pem \
  --key /etc/adb-remote-server/server.key
```

The server does not require Android SDK or `adb`.
