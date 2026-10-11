pub mod adb_remote {
    tonic::include_proto!("adb_remote");
}

pub use adb_remote::*;

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    #[test]
    fn adb_proxy_frame_round_trips() {
        let frame = AdbProxyFrame {
            lease_id: "lease".into(),
            stream_id: 7,
            payload: Some(adb_proxy_frame::Payload::Data(vec![1, 2, 3])),
        };
        let encoded = frame.encode_to_vec();
        let decoded = AdbProxyFrame::decode(encoded.as_slice()).unwrap();
        assert_eq!(decoded.lease_id, "lease");
        assert_eq!(decoded.stream_id, 7);
        assert!(
            matches!(decoded.payload, Some(adb_proxy_frame::Payload::Data(bytes)) if bytes == vec![1, 2, 3])
        );
    }
}
