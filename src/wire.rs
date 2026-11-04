use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Varint(u64),
    Bytes(Vec<u8>),
    Fixed32(u32),
    Fixed64(u64),
}

/// 字段号 → 该字段出现的**全部**值。
///
/// protobuf 的 repeated 字段在 wire 上就是同号字段重复出现，因此必须保留
/// 每个值；若退化成单值映射，多元素 repeated 字段只会剩下最后一个。
pub type Message = BTreeMap<u32, Vec<Value>>;

fn put_varint(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let b = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

// `| 0` 显式写出 varint 的 wire type，与其他分支保持对称；clippy 视其为恒等运算
#[allow(clippy::identity_op)]
pub fn encode(msg: &Message) -> Vec<u8> {
    let mut out = Vec::new();
    for (&field, values) in msg {
        // 同号字段按值逐个写出：repeated 字段在 wire 上就是重复的同号 key。
        for value in values {
            match value {
                Value::Varint(v) => {
                    put_varint(&mut out, ((field as u64) << 3) | 0);
                    put_varint(&mut out, *v);
                }
                Value::Bytes(b) => {
                    put_varint(&mut out, ((field as u64) << 3) | 2);
                    put_varint(&mut out, b.len() as u64);
                    out.extend_from_slice(b);
                }
                Value::Fixed32(v) => {
                    put_varint(&mut out, ((field as u64) << 3) | 5);
                    out.extend_from_slice(&v.to_le_bytes());
                }
                Value::Fixed64(v) => {
                    put_varint(&mut out, ((field as u64) << 3) | 1);
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
    }
    out
}

/// 从 `raw` 的 `i` 位置读一个 varint，返回 `(值, 新位置)`；数据不足返回 `None`。
///
/// 供 schema 层解包 packed repeated 标量使用（packed 负载里是连续裸 varint）。
pub fn read_varint_at(raw: &[u8], i: usize) -> Option<(u64, usize)> {
    let mut idx = i;
    let v = read_varint(raw, &mut idx)?;
    Some((v, idx))
}

fn read_varint(raw: &[u8], i: &mut usize) -> Option<u64> {
    let mut r = 0u64;
    let mut shift = 0u32;
    loop {
        let b = *raw.get(*i)?;
        *i += 1;
        r |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Some(r);
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
}

fn decode_inner(raw: &[u8]) -> (Message, Option<String>) {
    let mut out = Message::new();
    let mut i = 0usize;
    while i < raw.len() {
        let Some(key) = read_varint(raw, &mut i) else {
            return (out, Some("varint 键截断".to_string()));
        };
        let field_no = key >> 3;
        if field_no == 0 || field_no > 536_870_911 {
            return (out, Some(format!("字段号越界：{field_no}")));
        }
        let field = field_no as u32;
        let wire = (key & 7) as u8;
        let value = match wire {
            0 => match read_varint(raw, &mut i) {
                Some(v) => Value::Varint(v),
                None => return (out, Some("varint 值截断".to_string())),
            },
            2 => {
                let Some(len) = read_varint(raw, &mut i) else {
                    return (out, Some("length 前缀截断".to_string()));
                };
                let len = len as usize;
                // 用减法而非 `i + len` 比较：`len` 可达 u64::MAX，相加会溢出
                // （debug 报 overflow panic，release 回绕后切片越界 panic）。
                // `i` 恒 <= raw.len()，故 raw.len() - i 不会下溢。
                if len > raw.len() - i {
                    return (out, Some("length 越界".to_string()));
                }
                let v = raw[i..i + len].to_vec();
                i += len;
                Value::Bytes(v)
            }
            5 => {
                if i + 4 > raw.len() {
                    return (out, Some("fixed32 截断".to_string()));
                }
                let v = u32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
                i += 4;
                Value::Fixed32(v)
            }
            1 => {
                if i + 8 > raw.len() {
                    return (out, Some("fixed64 截断".to_string()));
                }
                let v = u64::from_le_bytes(raw[i..i + 8].try_into().unwrap());
                i += 8;
                Value::Fixed64(v)
            }
            other => return (out, Some(format!("不支持的 wire type：{other}"))),
        };
        // 同号字段追加而非覆盖：repeated 字段的每个元素都必须保留。
        out.entry(field).or_default().push(value);
    }
    (out, None)
}

/// 宽松解码：遇到畸形数据时保留已解析的部分。
///
/// 用于嵌套消息的 JSON 展示（schema 层没有错误通道）；网络响应必须用
/// [`decode_strict`]，否则截断响应会被误报为成功。
pub fn decode(raw: &[u8]) -> Message {
    decode_inner(raw).0
}

/// 严格解码：任何截断/越界/非法 wire type 都返回错误。
pub fn decode_strict(raw: &[u8]) -> Result<Message, String> {
    let (msg, err) = decode_inner(raw);
    match err {
        Some(e) => Err(e),
        None => Ok(msg),
    }
}
