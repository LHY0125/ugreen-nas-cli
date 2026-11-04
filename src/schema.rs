use crate::wire::{Message, Value};
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer};
use serde_json::{json, Map, Value as J};
use std::collections::BTreeMap;

/// 单个消息字段的定义。
#[derive(Debug, Clone, Deserialize)]
pub struct FieldDef {
    pub no: u32,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub type_name: Option<String>,
}

/// 方法元信息：套接字、操作类型与所需角色。
#[derive(Debug, Clone, Deserialize)]
pub struct MethodInfo {
    pub socket: String,
    pub operation: String,
    #[serde(default)]
    pub role: String,
}

/// 服务方法的签名（请求/响应消息全名、是否服务端流式）。
#[derive(Debug, Clone, Deserialize)]
pub struct MethodSig {
    #[serde(rename = "in")]
    pub input: String,
    #[serde(rename = "out")]
    pub output: String,
    #[serde(default)]
    pub server_streaming: bool,
}

/// 反序列化用的原始结构，字段全部可选以便容忍缺段。
#[derive(Debug, Deserialize)]
struct Raw {
    #[serde(default)]
    messages: BTreeMap<String, Vec<FieldDef>>,
    #[serde(default, deserialize_with = "de_enum_map")]
    enums: BTreeMap<String, BTreeMap<String, i64>>,
    #[serde(default)]
    services: BTreeMap<String, BTreeMap<String, MethodSig>>,
    #[serde(default)]
    methods: BTreeMap<String, MethodInfo>,
}

/// 枚举值在描述符里是 `int32`：负数被提取成了 u64 补码
/// （`-1` → `18446744073709551615`）。这里统一回绕成 `i64`，
/// 与 `Value::Varint(..) as i64` 的匹配方式保持一致。
fn de_enum_map<'de, D>(d: D) -> Result<BTreeMap<String, BTreeMap<String, i64>>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: BTreeMap<String, BTreeMap<String, J>> = BTreeMap::deserialize(d)?;
    let mut out = BTreeMap::new();
    for (enum_name, variants) in raw {
        let mut converted = BTreeMap::new();
        for (symbol, value) in variants {
            let no = value
                .as_i64()
                .or_else(|| value.as_u64().map(|v| v as i64))
                .ok_or_else(|| D::Error::custom(format!("枚举值不是整数：{enum_name}.{symbol}")))?;
            converted.insert(symbol, no);
        }
        out.insert(enum_name, converted);
    }
    Ok(out)
}

/// 从固件提取出的 protobuf schema。
#[derive(Debug)]
pub struct Schema {
    messages: BTreeMap<String, Vec<FieldDef>>,
    enums: BTreeMap<String, BTreeMap<String, i64>>,
    services: BTreeMap<String, BTreeMap<String, MethodSig>>,
    methods: BTreeMap<String, MethodInfo>,
}

/// 只有 `read` 视为只读；`write`/`delete`/`execute` 等都会改动设备状态。
pub fn is_read_only(operation: &str) -> bool {
    operation == "read"
}

impl Schema {
    pub fn from_json_str(text: &str) -> Result<Self, String> {
        let raw: Raw = serde_json::from_str(text).map_err(|e| e.to_string())?;
        Ok(Self {
            messages: raw.messages,
            enums: raw.enums,
            services: raw.services,
            methods: raw.methods,
        })
    }

    pub fn method(&self, path: &str) -> Option<&MethodInfo> {
        self.methods.get(path)
    }

    pub fn methods(&self) -> impl Iterator<Item = (&String, &MethodInfo)> {
        self.methods.iter()
    }

    /// 由 gRPC 方法路径（`/Service/Method`）查出请求消息全名。
    pub fn request_type(&self, method_path: &str) -> Option<String> {
        let rest = method_path.strip_prefix('/')?;
        let (service, method) = rest.split_once('/')?;
        self.services
            .get(service)?
            .get(method)
            .map(|sig| sig.input.clone())
    }

    /// 由 gRPC 方法路径（`/Service/Method`）查出响应消息全名。
    pub fn response_type(&self, method_path: &str) -> Option<String> {
        let rest = method_path.strip_prefix('/')?;
        let (service, method) = rest.split_once('/')?;
        self.services
            .get(service)?
            .get(method)
            .map(|sig| sig.output.clone())
    }

    /// 按字段名查字段号。命令表用名字而非数字，固件升级换号也不会错。
    pub fn field_no(&self, type_name: &str, field_name: &str) -> Option<u32> {
        self.messages
            .get(type_name)?
            .iter()
            .find(|f| f.name == field_name)
            .map(|f| f.no)
    }

    /// 按字段号把消息转成 JSON。未知字段号保留为 `field_<N>`。
    ///
    /// 同一字段号可能有多个值（repeated 字段）：多值输出 JSON 数组，
    /// 单值输出标量——避免把所有单值字段都变成数组，破坏既有输出契约。
    pub fn message_to_json(&self, type_name: &str, msg: &Message) -> J {
        let defs = self.messages.get(type_name);
        let mut out = Map::new();
        for (&no, values) in msg {
            if values.is_empty() {
                continue;
            }
            let def = defs.and_then(|d| d.iter().find(|f| f.no == no));
            let key = def
                .map(|d| d.name.clone())
                .unwrap_or_else(|| format!("field_{no}"));
            out.insert(key, self.values_to_json(def, values));
        }
        J::Object(out)
    }

    /// 把同号字段的全部值转成 JSON：单值出标量，多值出数组。
    fn values_to_json(&self, def: Option<&FieldDef>, values: &[Value]) -> J {
        match values {
            [] => J::Null,
            [single] => self.value_to_json(def, single),
            many => J::Array(many.iter().map(|v| self.value_to_json(def, v)).collect()),
        }
    }

    fn value_to_json(&self, def: Option<&FieldDef>, value: &Value) -> J {
        match value {
            Value::Varint(v) => {
                if let Some(d) = def {
                    if d.kind == "enum" {
                        if let Some(tn) = &d.type_name {
                            if let Some(sym) = self.enum_symbol(tn, *v as i64) {
                                return J::String(sym);
                            }
                        }
                    }
                    if d.kind == "bool" {
                        return json!(*v != 0);
                    }
                }
                json!(*v)
            }
            Value::Fixed64(v) => {
                if def.map(|d| d.kind == "double").unwrap_or(false) {
                    json!(f64::from_bits(*v))
                } else {
                    json!(v)
                }
            }
            Value::Fixed32(v) => {
                if def.map(|d| d.kind == "float").unwrap_or(false) {
                    json!(f32::from_bits(*v))
                } else {
                    json!(v)
                }
            }
            Value::Bytes(b) => {
                if let Some(tn) = def.and_then(|d| d.type_name.clone()) {
                    if self.messages.contains_key(&tn) {
                        let nested = crate::wire::decode(b);
                        return self.message_to_json(&tn, &nested);
                    }
                }
                match std::str::from_utf8(b) {
                    Ok(s) => json!(s),
                    Err(_) => json!(b.iter().map(|x| format!("{x:02x}")).collect::<String>()),
                }
            }
        }
    }

    fn enum_symbol(&self, type_name: &str, value: i64) -> Option<String> {
        let m = self.enums.get(type_name)?;
        m.iter().find(|(_, &v)| v == value).map(|(k, _)| k.clone())
    }
}

/// 便捷入口：不指定消息类型时按字段号裸转，供 `call` 兜底命令使用。
pub fn to_json(msg: &Message, schema: &Schema) -> J {
    let mut out = Map::new();
    for (&no, values) in msg {
        if values.is_empty() {
            continue;
        }
        let key = format!("field_{no}");
        out.insert(key, schema.values_to_json(None, values));
    }
    J::Object(out)
}
