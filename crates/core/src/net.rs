use anyhow::Result;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::mpsc,
};

pub const DEFAULT_ADB_SERVER: &str = "127.0.0.1:5037";

pub async fn read_to_channel<R>(mut reader: R, tx: mpsc::Sender<Vec<u8>>) -> Result<u64>
where
    R: AsyncRead + Unpin,
{
    let mut total = 0;
    let mut buf = vec![0_u8; 16 * 1024];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if tx.send(buf[..n].to_vec()).await.is_err() {
            break;
        }
    }
    Ok(total)
}

pub async fn write_from_channel<W>(mut writer: W, mut rx: mpsc::Receiver<Vec<u8>>) -> Result<u64>
where
    W: AsyncWrite + Unpin,
{
    let mut total = 0;
    while let Some(bytes) = rx.recv().await {
        writer.write_all(&bytes).await?;
        total += bytes.len() as u64;
    }
    writer.shutdown().await.ok();
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn channel_pumps_bytes_through_tcp_echo() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (mut reader, mut writer) = socket.split();
            tokio::io::copy(&mut reader, &mut writer).await.unwrap();
        });

        let socket = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (reader, writer) = socket.into_split();
        let (to_socket_tx, to_socket_rx) = mpsc::channel(4);
        let (from_socket_tx, mut from_socket_rx) = mpsc::channel(4);

        let write_task = tokio::spawn(write_from_channel(writer, to_socket_rx));
        let read_task = tokio::spawn(read_to_channel(reader, from_socket_tx));

        to_socket_tx.send(b"hello".to_vec()).await.unwrap();
        drop(to_socket_tx);

        let echoed = from_socket_rx.recv().await.unwrap();
        assert_eq!(echoed, b"hello");
        drop(from_socket_rx);

        assert_eq!(write_task.await.unwrap().unwrap(), 5);
        assert!(read_task.await.unwrap().unwrap() >= 5);
    }
}
