use serde_json::{Value, json};
use std::io::Read;
pub fn messages(bytes: &[u8], limit: usize) -> Result<Vec<Value>, String> {
    let mut output = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() && output.len() < limit {
        let size = u32::from_be_bytes(
            bytes
                .get(offset..offset + 4)
                .ok_or("RocketMQ 消息长度截断")?
                .try_into()
                .unwrap(),
        ) as usize;
        let message = bytes
            .get(offset..offset + size)
            .ok_or("RocketMQ 消息内容截断")?;
        if size < 88 {
            return Err("RocketMQ 消息长度无效".into());
        }
        offset += size;
        let be32 = |n| u32::from_be_bytes(message[n..n + 4].try_into().unwrap());
        let be64 = |n| u64::from_be_bytes(message[n..n + 8].try_into().unwrap());
        let queue = be32(12);
        let queue_offset = be64(20);
        let flags = be32(36);
        let host1 = if flags & (1 << 4) != 0 { 20 } else { 8 };
        let host2 = if flags & (1 << 5) != 0 { 20 } else { 8 };
        let position = 40 + 8 + host1 + 8 + host2 + 4 + 8;
        let length_bytes = message
            .get(position..position + 4)
            .ok_or("RocketMQ 消息体长度截断")?;
        let length = u32::from_be_bytes(length_bytes.try_into().unwrap()) as usize;
        let body = message
            .get(position + 4..position + 4 + length)
            .ok_or("RocketMQ 消息体截断")?;
        let body = if flags & 1 != 0 {
            let mut decoded = Vec::new();
            flate2::read::ZlibDecoder::new(body)
                .take((1 << 20) + 1)
                .read_to_end(&mut decoded)
                .map_err(|_| "RocketMQ 消息解压失败")?;
            if decoded.len() > 1 << 20 {
                return Err("RocketMQ 解压消息超限".into());
            }
            decoded
        } else {
            body.to_vec()
        };
        let position = position + 4 + length;
        let topic_size = if be32(4) == 0xdaa320ab { 2 } else { 1 };
        let topic_length = if topic_size == 2 {
            u16::from_be_bytes(
                message
                    .get(position..position + 2)
                    .ok_or("RocketMQ 主题长度截断")?
                    .try_into()
                    .unwrap(),
            ) as usize
        } else {
            *message.get(position).ok_or("RocketMQ 主题长度截断")? as usize
        };
        let topic = message
            .get(position + topic_size..position + topic_size + topic_length)
            .ok_or("RocketMQ 主题截断")?;
        output.push(json!({"queueId":queue,"queueOffset":queue_offset,"topic":String::from_utf8_lossy(topic),"payload":String::from_utf8_lossy(&body)}));
    }
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_truncated_or_oversize_frames() {
        for data in [vec![], vec![0], vec![0, 0, 1, 0], vec![0; 88]] {
            if !data.is_empty() {
                assert!(messages(&data, 1).is_err());
            }
        }
    }
}
