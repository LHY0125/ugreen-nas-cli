# nascli 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把绿联 UGOS Pro 的 OpenCLI 访问令牌接口封装成一个默认只读、可安全交给 AI 智能体使用的 Rust CLI，并配一个 Claude Code 技能。

**Architecture:** 单 crate。核心是一个**通用 protobuf 编解码器**（`wire.rs`）+ 一份**从官方二进制提取的 schema**（`spec/schema.json`，编译期嵌入）。命令表静态声明「gRPC 方法 → 参数字段号」的映射，因此 CLI 代码不含任何硬编码字段号，固件升级只需重跑提取器。

**Tech Stack:** Rust 2021（单 crate）、`clap` 4（derive）、`reqwest` 0.12（blocking）、`serde` / `serde_json`。提取器为 Python 3（仅标准库）。

**Spec:** `docs/design.md`

## Global Constraints

以下约束适用于**每一个任务**，不再逐条重复：

- 单 crate，`edition = "2021"`，不使用 workspace。
- **不得出现任何真实环境信息**：主机名、IP、序列号、用户名、邮箱、路径。示例统一用 `nas.example.com` / `/volume1/demo`。
- `UGNAS_HOST` 与 `UGPOA_TOKEN` **无默认值**，缺失即报错退出（退出码 2）。
- **stdout 只输出 JSON**；人类可读信息一律走 stderr。
- 退出码固定：`0` 成功 / `2` 参数错误 / `3` 未授权 / `4` 无权限 / `5` 方法不支持 / `6` 网络错误 / `7` 服务端错误。
- 写类操作（`write`/`delete`/`execute`）无 `--yes` 必须拒绝执行。
- `spec/` 进 `.gitignore`，不得入库。
- 令牌在日志与错误信息中一律脱敏为 `ugpoa_v1.****`。
- 不使用 `tonic` / `prost`。

---

### Task 1: 项目骨架与构建守卫

**Files:**
- Create: `Cargo.toml`
- Create: `build.rs`
- Create: `.gitignore`
- Create: `src/main.rs`
- Create: `spec/.gitkeep`

**Interfaces:**
- Consumes: 无
- Produces: 可编译的 crate；`build.rs` 在 `spec/schema.json` 缺失时中止构建

- [ ] **Step 1: 写 `Cargo.toml`**

```toml
[package]
name = "nascli"
version = "0.1.0"
edition = "2021"
description = "CLI for UGOS Pro OpenCLI access-token API"
license = "MIT"

[dependencies]
clap = { version = "4", features = ["derive"] }
reqwest = { version = "0.12", features = ["blocking", "json", "rustls-tls"], default-features = false }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: 写 `build.rs`（构建守卫）**

```rust
use std::path::Path;

fn main() {
    let schema = Path::new("spec/schema.json");
    println!("cargo:rerun-if-changed=spec/schema.json");
    if !schema.exists() {
        panic!(
            "\n\n缺少 spec/schema.json。\n\
             请先运行：python tools/extract_schema.py --host <NAS 地址>\n\
             （需要 UGPOA_TOKEN 与到 NAS 的 SSH 访问）\n"
        );
    }
}
```

- [ ] **Step 3: 写 `.gitignore`**

```gitignore
/target
/spec/schema.json
/spec/openapi/
```

- [ ] **Step 4: 写最小 `src/main.rs`**

```rust
fn main() {
    println!("{}", serde_json::json!({"ok": true, "data": {}}));
}
```

- [ ] **Step 5: 验证构建守卫会拦住缺失的 schema**

```bash
cargo build 2>&1 | grep -q "缺少 spec/schema.json" && echo "GUARD OK"
```
Expected: 输出 `GUARD OK`

- [ ] **Step 6: 造一个占位 schema 并验证能编译**

```bash
echo '{}' > spec/schema.json
cargo build
```
Expected: 编译成功

- [ ] **Step 7: 提交**

```bash
git add Cargo.toml build.rs .gitignore src/main.rs
git commit -m "chore: scaffold nascli crate with build-time schema guard"
```

---

### Task 2: `wire.rs` — 通用 protobuf 编解码

**Files:**
- Create: `src/wire.rs`
- Modify: `src/main.rs`（加 `mod wire;`）
- Test: `tests/wire.rs`

**Interfaces:**
- Consumes: 无
- Produces:
  - `pub enum Value { Varint(u64), Bytes(Vec<u8>), Fixed32(u32), Fixed64(u64) }`
  - `pub type Message = std::collections::BTreeMap<u32, Value>;`
  - `pub fn encode(msg: &Message) -> Vec<u8>`
  - `pub fn decode(raw: &[u8]) -> Message`

- [ ] **Step 1: 写失败测试 `tests/wire.rs`**

```rust
use nascli::wire::{decode, encode, Message, Value};
use std::collections::BTreeMap;

fn msg(pairs: &[(u32, Value)]) -> Message {
    pairs.iter().cloned().collect::<BTreeMap<_, _>>()
}

#[test]
fn roundtrip_string_field() {
    let m = msg(&[(3, Value::Bytes(b"/volume1/demo".to_vec()))]);
    assert_eq!(decode(&encode(&m)), m);
}

#[test]
fn roundtrip_varint_field() {
    let m = msg(&[(8, Value::Varint(500))]);
    assert_eq!(decode(&encode(&m)), m);
}

#[test]
fn encodes_known_bytes() {
    // field 1, wire type 2, len 1, "A"
    let m = msg(&[(1, Value::Bytes(b"A".to_vec()))]);
    assert_eq!(encode(&m), vec![0x0a, 0x01, 0x41]);
}

#[test]
fn varint_boundaries() {
    for v in [0u64, 1, 127, 128, 16383, 16384, u32::MAX as u64, u64::MAX] {
        let m = msg(&[(1, Value::Varint(v))]);
        assert_eq!(decode(&encode(&m)), m, "failed at {v}");
    }
}

#[test]
fn empty_message_roundtrips() {
    assert_eq!(decode(&encode(&Message::new())), Message::new());
}

#[test]
fn decodes_nested_bytes_as_opaque() {
    // 嵌套消息在 wire 层就是 Bytes，展开由 schema 层负责
    let inner = encode(&msg(&[(1, Value::Bytes(b"x".to_vec()))]));
    let outer = msg(&[(2, Value::Bytes(inner))]);
    assert_eq!(decode(&encode(&outer)), outer);
}

#[test]
fn repeated_field_last_wins() {
    // 同一字段号出现两次时，BTreeMap 保留最后一个
    let raw = vec![0x08, 0x01, 0x08, 0x02];
    assert_eq!(decode(&raw).get(&1), Some(&Value::Varint(2)));
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test --test wire
```
Expected: 编译失败，`unresolved import nascli::wire`

- [ ] **Step 3: 加 `src/lib.rs` 并实现 `src/wire.rs`**

`src/lib.rs`:
```rust
pub mod wire;
```

`src/wire.rs`:
```rust
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Varint(u64),
    Bytes(Vec<u8>),
    Fixed32(u32),
    Fixed64(u64),
}

pub type Message = BTreeMap<u32, Value>;

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

pub fn encode(msg: &Message) -> Vec<u8> {
    let mut out = Vec::new();
    for (&field, value) in msg {
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
    out
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

pub fn decode(raw: &[u8]) -> Message {
    let mut out = Message::new();
    let mut i = 0usize;
    while i < raw.len() {
        let Some(key) = read_varint(raw, &mut i) else { break };
        let field = (key >> 3) as u32;
        let wire = (key & 7) as u8;
        if field == 0 {
            break;
        }
        let value = match wire {
            0 => match read_varint(raw, &mut i) {
                Some(v) => Value::Varint(v),
                None => break,
            },
            2 => {
                let Some(len) = read_varint(raw, &mut i) else { break };
                let len = len as usize;
                if i + len > raw.len() {
                    break;
                }
                let v = raw[i..i + len].to_vec();
                i += len;
                Value::Bytes(v)
            }
            5 => {
                if i + 4 > raw.len() {
                    break;
                }
                let v = u32::from_le_bytes(raw[i..i + 4].try_into().unwrap());
                i += 4;
                Value::Fixed32(v)
            }
            1 => {
                if i + 8 > raw.len() {
                    break;
                }
                let v = u64::from_le_bytes(raw[i..i + 8].try_into().unwrap());
                i += 8;
                Value::Fixed64(v)
            }
            _ => break,
        };
        out.insert(field, value);
    }
    out
}
```

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test --test wire
```
Expected: 7 passed

- [ ] **Step 5: 提交**

```bash
git add src/lib.rs src/wire.rs tests/wire.rs
git commit -m "feat(wire): generic protobuf encode/decode"
```

---

### Task 3: `schema.rs` — schema 加载与 JSON 化

**Files:**
- Create: `src/schema.rs`
- Modify: `src/lib.rs`
- Test: `tests/schema.rs`

**Interfaces:**
- Consumes: `wire::{Message, Value}`
- Produces:
  - `pub struct Schema`，`Schema::from_json_str(&str) -> Result<Schema, String>`
  - `pub struct MethodInfo { pub socket: String, pub operation: String, pub role: String }`
  - `impl Schema { pub fn method(&self, path: &str) -> Option<&MethodInfo>; pub fn methods(&self) -> impl Iterator<Item = (&String, &MethodInfo)>; }`
  - `pub fn request_type(&self, method_path: &str) -> Option<String>` —— 由 gRPC 方法路径查出请求消息全名
  - `pub fn field_no(&self, type_name: &str, field_name: &str) -> Option<u32>` —— 按字段名查字段号
  - `pub fn to_json(msg: &Message, schema: &Schema) -> serde_json::Value`
  - `pub fn is_read_only(op: &str) -> bool`

- [ ] **Step 1: 写失败测试 `tests/schema.rs`**

```rust
use nascli::schema::{is_read_only, to_json, Schema};
use nascli::wire::{Message, Value};

const SAMPLE: &str = r#"{
  "messages": {
    "demo.Req": [
      {"no": 3, "name": "path", "type": "string"},
      {"no": 8, "name": "limit", "type": "int32"},
      {"no": 16, "name": "mode", "type": "enum", "type_name": "demo.Mode"}
    ]
  },
  "enums": { "demo.Mode": { "FAST": 0, "SAFE": 1 } },
  "services": {
    "storage.Storage": {
      "VolumeList": {"in": "demo.Req", "out": "demo.Resp", "server_streaming": false}
    }
  },
  "methods": {
    "/storage.Storage/VolumeList": {"socket": "ugreen.storage.grpc", "operation": "read", "role": "admin"},
    "/ugidl.filesystem.Filesystem/DelPath": {"socket": "ugreen.filemgr.internal.grpc", "operation": "delete", "role": "user"}
  }
}"#;

#[test]
fn parses_methods() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    let m = s.method("/storage.Storage/VolumeList").unwrap();
    assert_eq!(m.socket, "ugreen.storage.grpc");
    assert_eq!(m.operation, "read");
}

#[test]
fn read_only_classification() {
    assert!(is_read_only("read"));
    assert!(!is_read_only("write"));
    assert!(!is_read_only("delete"));
    assert!(!is_read_only("execute"));
}

#[test]
fn maps_field_numbers_to_names() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    let mut m = Message::new();
    m.insert(3, Value::Bytes(b"/volume1/demo".to_vec()));
    m.insert(8, Value::Varint(50));
    let j = s.message_to_json("demo.Req", &m);
    assert_eq!(j["path"], "/volume1/demo");
    assert_eq!(j["limit"], 50);
}

#[test]
fn resolves_enum_symbol() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    let mut m = Message::new();
    m.insert(16, Value::Varint(1));
    assert_eq!(s.message_to_json("demo.Req", &m)["mode"], "SAFE");
}

#[test]
fn unknown_field_is_preserved() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    let mut m = Message::new();
    m.insert(99, Value::Varint(7));
    assert_eq!(to_json(&m, &s)["field_99"], 7);
}

#[test]
fn rejects_malformed_json() {
    assert!(Schema::from_json_str("not json").is_err());
}

#[test]
fn resolves_request_type_from_method() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    assert_eq!(
        s.request_type("/storage.Storage/VolumeList").as_deref(),
        Some("demo.Req")
    );
    assert!(s.request_type("/nope.Nope/Nope").is_none());
    assert!(s.request_type("malformed").is_none());
}

#[test]
fn looks_up_field_number_by_name() {
    let s = Schema::from_json_str(SAMPLE).unwrap();
    assert_eq!(s.field_no("demo.Req", "path"), Some(3));
    assert_eq!(s.field_no("demo.Req", "limit"), Some(8));
    assert_eq!(s.field_no("demo.Req", "missing"), None);
    assert_eq!(s.field_no("no.Such", "path"), None);
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test --test schema
```
Expected: 编译失败，`unresolved import nascli::schema`

- [ ] **Step 3: 实现 `src/schema.rs`**

```rust
use crate::wire::{Message, Value};
use serde::Deserialize;
use serde_json::{json, Map, Value as J};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
pub struct FieldDef {
    pub no: u32,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub type_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MethodInfo {
    pub socket: String,
    pub operation: String,
    #[serde(default)]
    pub role: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MethodSig {
    #[serde(rename = "in")]
    pub input: String,
    #[serde(rename = "out")]
    pub output: String,
    #[serde(default)]
    pub server_streaming: bool,
}

#[derive(Debug, Deserialize)]
struct Raw {
    #[serde(default)]
    messages: BTreeMap<String, Vec<FieldDef>>,
    #[serde(default)]
    enums: BTreeMap<String, BTreeMap<String, i64>>,
    #[serde(default)]
    services: BTreeMap<String, BTreeMap<String, MethodSig>>,
    #[serde(default)]
    methods: BTreeMap<String, MethodInfo>,
}

#[derive(Debug)]
pub struct Schema {
    messages: BTreeMap<String, Vec<FieldDef>>,
    enums: BTreeMap<String, BTreeMap<String, i64>>,
    services: BTreeMap<String, BTreeMap<String, MethodSig>>,
    methods: BTreeMap<String, MethodInfo>,
}

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

    /// 按字段名查字段号。命令表用名字而非数字，固件升级换号也不会错。
    pub fn field_no(&self, type_name: &str, field_name: &str) -> Option<u32> {
        self.messages
            .get(type_name)?
            .iter()
            .find(|f| f.name == field_name)
            .map(|f| f.no)
    }

    /// 按字段号把消息转成 JSON。未知字段号保留为 `field_<N>`。
    pub fn message_to_json(&self, type_name: &str, msg: &Message) -> J {
        let defs = self.messages.get(type_name);
        let mut out = Map::new();
        for (&no, value) in msg {
            let def = defs.and_then(|d| d.iter().find(|f| f.no == no));
            let key = def
                .map(|d| d.name.clone())
                .unwrap_or_else(|| format!("field_{no}"));
            out.insert(key, self.value_to_json(def, value));
        }
        J::Object(out)
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
                    if d.kind == "double" {
                        return json!(f64::from_bits(*v));
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
            Value::Fixed32(v) => json!(v),
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
        m.iter()
            .find(|(_, &v)| v == value)
            .map(|(k, _)| k.clone())
    }
}

/// 便捷入口：不指定消息类型时按字段号裸转。
pub fn to_json(msg: &Message, schema: &Schema) -> J {
    let mut out = Map::new();
    for (&no, value) in msg {
        let key = format!("field_{no}");
        let v = schema.value_to_json(None, value);
        out.insert(key, v);
    }
    J::Object(out)
}
```

> 注意：`to_json` 是「无类型」入口，用于 `call` 兜底；语义命令用 `Schema::message_to_json`，能拿到真实字段名。

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test --test schema
```
Expected: 8 passed

- [ ] **Step 5: 提交**

```bash
git add src/schema.rs src/lib.rs tests/schema.rs
git commit -m "feat(schema): load schema and map protobuf fields to JSON"
```

---

### Task 4: `config.rs` — 配置解析

**Files:**
- Create: `src/config.rs`
- Modify: `src/lib.rs`
- Test: `tests/config.rs`

**Interfaces:**
- Consumes: 无
- Produces:
  - `pub struct Config { pub host: String, pub port: u16, pub token: String, pub insecure: bool }`
  - `impl Config { pub fn base_url(&self) -> String; pub fn from_env() -> Result<Config, ConfigError> }`
  - `pub enum ConfigError { MissingHost, MissingToken, BadPort(String) }`，实现 `Display`
  - `pub fn redact(token: &str) -> String`

- [ ] **Step 1: 写失败测试 `tests/config.rs`**

```rust
use nascli::config::{redact, Config, ConfigError};

fn cfg(host: &str, token: &str) -> Config {
    Config { host: host.into(), port: 9443, token: token.into(), insecure: true }
}

#[test]
fn builds_base_url() {
    assert_eq!(cfg("nas.example.com", "t").base_url(),
               "https://nas.example.com:9443");
}

#[test]
fn redacts_token() {
    assert_eq!(redact("ugpoa_v1.abcdef0123456789.secret"), "ugpoa_v1.****");
    assert_eq!(redact("short"), "****");
}

#[test]
fn missing_host_is_an_error() {
    let e = Config::from_parts(None, None, None, None).unwrap_err();
    assert!(matches!(e, ConfigError::MissingHost));
}

#[test]
fn missing_token_is_an_error() {
    let e = Config::from_parts(Some("nas.example.com".into()), None, None, None).unwrap_err();
    assert!(matches!(e, ConfigError::MissingToken));
}

#[test]
fn defaults_port_and_insecure() {
    let c = Config::from_parts(Some("nas.example.com".into()), Some("tok".into()), None, None).unwrap();
    assert_eq!(c.port, 9443);
    assert!(c.insecure);
}

#[test]
fn insecure_can_be_disabled() {
    let c = Config::from_parts(Some("h".into()), Some("t".into()), None, Some("0".into())).unwrap();
    assert!(!c.insecure);
}

#[test]
fn bad_port_is_an_error() {
    let e = Config::from_parts(Some("h".into()), Some("t".into()), Some("abc".into()), None).unwrap_err();
    assert!(matches!(e, ConfigError::BadPort(_)));
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test --test config
```
Expected: 编译失败，`unresolved import nascli::config`

- [ ] **Step 3: 实现 `src/config.rs`**

```rust
use std::fmt;

#[derive(Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub insecure: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    MissingHost,
    MissingToken,
    BadPort(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MissingHost => write!(
                f,
                "缺少环境变量 UGNAS_HOST。示例：\n  \
                 $env:UGNAS_HOST = \"nas.example.com\""
            ),
            ConfigError::MissingToken => write!(
                f,
                "缺少环境变量 UGPOA_TOKEN。请在 NAS 的「个人中心 → 本地账号 → 访问令牌」创建。"
            ),
            ConfigError::BadPort(v) => write!(f, "UGNAS_PORT 不是合法端口：{v}"),
        }
    }
}

/// 令牌脱敏：只保留前缀，其余用 **** 代替。
pub fn redact(token: &str) -> String {
    match token.split_once('.') {
        Some((head, _)) => format!("{head}.****"),
        None => "****".to_string(),
    }
}

impl Config {
    pub fn base_url(&self) -> String {
        format!("https://{}:{}", self.host, self.port)
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_parts(
            std::env::var("UGNAS_HOST").ok(),
            std::env::var("UGPOA_TOKEN").ok(),
            std::env::var("UGNAS_PORT").ok(),
            std::env::var("UGNAS_INSECURE").ok(),
        )
    }

    pub fn from_parts(
        host: Option<String>,
        token: Option<String>,
        port: Option<String>,
        insecure: Option<String>,
    ) -> Result<Self, ConfigError> {
        let host = host.filter(|h| !h.trim().is_empty()).ok_or(ConfigError::MissingHost)?;
        let token = token.filter(|t| !t.trim().is_empty()).ok_or(ConfigError::MissingToken)?;
        let port = match port {
            Some(p) => p.parse::<u16>().map_err(|_| ConfigError::BadPort(p))?,
            None => 9443,
        };
        let insecure = insecure.map(|v| v != "0").unwrap_or(true);
        Ok(Self { host, port, token, insecure })
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test --test config
```
Expected: 7 passed

- [ ] **Step 5: 提交**

```bash
git add src/config.rs src/lib.rs tests/config.rs
git commit -m "feat(config): env-based configuration with token redaction"
```

---

### Task 5: `client.rs` — HTTP 调用与错误映射

**Files:**
- Create: `src/client.rs`
- Modify: `src/lib.rs`
- Test: `tests/client.rs`

**Interfaces:**
- Consumes: `config::Config`、`wire::{encode, decode, Message}`
- Produces:
  - `pub struct Client { .. }`，`Client::new(Config) -> Result<Client, String>`
  - `pub fn call(&self, socket: &str, method: &str, msg: &Message) -> Result<Message, ApiError>`
  - `pub enum ApiError { Unauthorized, Forbidden(String), BadRequest(String), Unsupported(String), Network(String), Server(String) }`
  - `impl ApiError { pub fn exit_code(&self) -> i32; pub fn code(&self) -> &'static str; pub fn message(&self) -> &str }`
  - `pub fn classify(status: u16, grpc_code: Option<i64>, body: &str) -> ApiError`

- [ ] **Step 1: 写失败测试 `tests/client.rs`**

```rust
use nascli::client::{classify, ApiError};

#[test]
fn unauthorized_maps_to_3() {
    let e = classify(401, None, r#"{"code":"unauthorized"}"#);
    assert!(matches!(e, ApiError::Unauthorized));
    assert_eq!(e.exit_code(), 3);
    assert_eq!(e.code(), "UNAUTHORIZED");
}

#[test]
fn access_denied_maps_to_4() {
    let e = classify(500, Some(7), r#"{"message":"Access not allowed"}"#);
    assert_eq!(e.exit_code(), 4);
    assert_eq!(e.code(), "ACCESS_DENIED");
}

#[test]
fn cardinality_violation_means_streaming_unsupported() {
    let e = classify(500, Some(13),
        r#"{"message":"cardinality violation: expected <EOF> for non server-streaming RPCs"}"#);
    assert!(matches!(e, ApiError::Unsupported(_)));
    assert_eq!(e.exit_code(), 5);
}

#[test]
fn invalid_argument_maps_to_2() {
    let e = classify(400, Some(3), r#"{"message":"path is empty"}"#);
    assert_eq!(e.exit_code(), 2);
    assert_eq!(e.code(), "BAD_REQUEST");
}

#[test]
fn unknown_server_error_maps_to_7() {
    let e = classify(500, Some(2), r#"{"message":"not found app"}"#);
    assert_eq!(e.exit_code(), 7);
    assert_eq!(e.code(), "SERVER_ERROR");
}

#[test]
fn message_is_extracted_from_body() {
    let e = classify(400, Some(3), r#"{"message":"path is empty"}"#);
    assert_eq!(e.message(), "path is empty");
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test --test client
```
Expected: 编译失败，`unresolved import nascli::client`

- [ ] **Step 3: 实现 `src/client.rs`**

```rust
use crate::config::{redact, Config};
use crate::wire::{decode, encode, Message};
use std::time::Duration;

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    Forbidden(String),
    BadRequest(String),
    Unsupported(String),
    Network(String),
    Server(String),
}

impl ApiError {
    pub fn exit_code(&self) -> i32 {
        match self {
            ApiError::BadRequest(_) => 2,
            ApiError::Unauthorized => 3,
            ApiError::Forbidden(_) => 4,
            ApiError::Unsupported(_) => 5,
            ApiError::Network(_) => 6,
            ApiError::Server(_) => 7,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            ApiError::BadRequest(_) => "BAD_REQUEST",
            ApiError::Unauthorized => "UNAUTHORIZED",
            ApiError::Forbidden(_) => "ACCESS_DENIED",
            ApiError::Unsupported(_) => "UNSUPPORTED",
            ApiError::Network(_) => "NETWORK_ERROR",
            ApiError::Server(_) => "SERVER_ERROR",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            ApiError::BadRequest(m)
            | ApiError::Forbidden(m)
            | ApiError::Unsupported(m)
            | ApiError::Network(m)
            | ApiError::Server(m) => m,
            ApiError::Unauthorized => "令牌无效或已过期",
        }
    }
}

/// 从响应体里取 `message` 字段，取不到就返回原文（截断）。
fn extract_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(m) = v.get("message").and_then(|m| m.as_str()) {
            return m.to_string();
        }
        if let Some(m) = v.get("msg").and_then(|m| m.as_str()) {
            if !m.is_empty() {
                return m.to_string();
            }
        }
    }
    body.chars().take(200).collect()
}

pub fn classify(status: u16, grpc_code: Option<i64>, body: &str) -> ApiError {
    let msg = extract_message(body);
    if status == 401 {
        return ApiError::Unauthorized;
    }
    if msg.contains("cardinality violation") {
        return ApiError::Unsupported(
            "该方法为服务端流式 RPC，OpenCLI 的 HTTP 接口不支持".to_string(),
        );
    }
    if msg.contains("Access not allowed") {
        return ApiError::Forbidden(msg);
    }
    if status == 400 || grpc_code == Some(3) || grpc_code == Some(1005) {
        return ApiError::BadRequest(msg);
    }
    if status == 415 {
        return ApiError::Server(msg);
    }
    if status >= 500 {
        return ApiError::Server(msg);
    }
    ApiError::Server(msg)
}

pub struct Client {
    http: reqwest::blocking::Client,
    base: String,
    token: String,
}

impl Client {
    pub fn new(cfg: Config) -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            .danger_accept_invalid_certs(cfg.insecure)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { http, base: cfg.base_url(), token: cfg.token })
    }

    pub fn call(&self, socket: &str, method: &str, msg: &Message) -> Result<Message, ApiError> {
        let url = format!("{}/ugreen/opencli/v1/{}{}", self.base, socket, method);
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/x-protobuf")
            .body(encode(msg))
            .send()
            .map_err(|e| {
                let hint = if e.is_timeout() { "（超时）" } else { "" };
                ApiError::Network(format!("{e}{hint}"))
            })?;

        let status = resp.status().as_u16();
        let grpc_code = resp
            .headers()
            .get("x-grpc-code")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok());
        let bytes = resp.bytes().map_err(|e| ApiError::Network(e.to_string()))?;

        if status == 200 && grpc_code == Some(0) {
            return Ok(decode(&bytes));
        }
        let body = String::from_utf8_lossy(&bytes).to_string();
        Err(classify(status, grpc_code, &body))
    }

    /// 供错误信息使用，避免把令牌写进日志。
    pub fn redacted_token(&self) -> String {
        redact(&self.token)
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test --test client
```
Expected: 6 passed

- [ ] **Step 5: 提交**

```bash
git add src/client.rs src/lib.rs tests/client.rs
git commit -m "feat(client): HTTP transport with stable error classification"
```

---

### Task 6: `tools/extract_schema.py` — 描述符提取器

**Files:**
- Create: `tools/extract_schema.py`
- Test: `tools/tests/test_extract.py`

**Interfaces:**
- Consumes: 无
- Produces: `spec/schema.json`，结构见 `docs/design.md` §5.1
- CLI：`python tools/extract_schema.py --host <NAS> [--ssh-user <u>] [--out spec/schema.json] [--local-root <dir>]`

**关键算法（已在真机验证）：**
1. 通过 SSH 在 NAS 上扫描二进制（服务端 `sbin/*` 与 CLI `bin/*`）。
2. 用正则找候选 `.proto` 名，再按 `0x0A <len> <name>` 定位描述符起点。
3. **解析必须限定边界**（描述符在二进制里没有长度前缀，否则会越界读到相邻数据）：解析时遇到 `field == 0 || field > 20` 即停止，且长度字段越界即停止。
4. **校验**：解析出的 `name` 必须等于目标名，否则视为假阳性，继续向后搜索。
5. 合并所有描述符的 messages / enums / services，再从各应用的 `openapi.json` 补充 `methods`（socket / operation / role）。

- [ ] **Step 1: 写失败测试 `tools/tests/test_extract.py`**

```python
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from extract_schema import parse_message, find_descriptors, build_schema  # noqa: E402


def varint(n: int) -> bytes:
    out = b""
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out += bytes([b | 0x80])
        else:
            return out + bytes([b])


def field_bytes(no: int, raw: bytes) -> bytes:
    return varint((no << 3) | 2) + varint(len(raw)) + raw


def make_descriptor(name: str, package: str) -> bytes:
    return field_bytes(1, name.encode()) + field_bytes(2, package.encode())


def test_parse_message_reads_name_and_package():
    raw = make_descriptor("demo/demo.proto", "demo.v1")
    parsed = parse_message(raw, 0, len(raw))
    assert parsed[1][0] == b"demo/demo.proto"
    assert parsed[2][0] == b"demo.v1"


def test_parse_message_stops_at_invalid_field_number():
    raw = make_descriptor("demo/demo.proto", "demo.v1") + varint((99 << 3) | 0) + varint(1)
    parsed = parse_message(raw, 0, len(raw))
    assert 99 not in parsed


def test_find_descriptors_skips_false_positives():
    good = make_descriptor("demo/demo.proto", "demo.v1")
    blob = b"\x00" * 8 + good + b"\x00" * 8
    found = find_descriptors(blob, ["demo/demo.proto"])
    assert "demo/demo.proto" in found


def test_build_schema_shape():
    schema = build_schema({}, {}, {}, {
        "/storage.Storage/VolumeList": {
            "socket": "ugreen.storage.grpc", "operation": "read", "role": "admin"
        }
    })
    assert schema["methods"]["/storage.Storage/VolumeList"]["operation"] == "read"
    assert set(schema) == {"messages", "enums", "services", "methods"}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cd tools && python -m pytest tests/test_extract.py -v
```
Expected: `ModuleNotFoundError: No module named 'extract_schema'`

- [ ] **Step 3: 实现 `tools/extract_schema.py`**

```python
#!/usr/bin/env python3
"""从绿联官方二进制中提取 protobuf 描述符，生成 spec/schema.json。

只读操作：仅通过 SSH 读取 NAS 上的文件，不修改任何配置。
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

PROTO_RE = re.compile(rb"[a-zA-Z0-9_/.-]{3,80}\.proto")
MAX_DESC_FIELD = 20  # FileDescriptorProto 最大字段号（edition=14），超过即视为越界


def read_varint(buf: bytes, i: int) -> tuple[int, int] | None:
    r = 0
    shift = 0
    while True:
        if i >= len(buf):
            return None
        b = buf[i]
        i += 1
        r |= (b & 0x7F) << shift
        if not b & 0x80:
            return r, i
        shift += 7
        if shift >= 64:
            return None


def parse_message(buf: bytes, i: int, end: int) -> dict[int, list]:
    """解析 protobuf 消息。遇到非法字段号或越界长度即停止。"""
    out: dict[int, list] = {}
    while i < end:
        got = read_varint(buf, i)
        if got is None:
            break
        key, i = got
        field, wire = key >> 3, key & 7
        if field == 0 or field > MAX_DESC_FIELD:
            break
        if wire == 0:
            got = read_varint(buf, i)
            if got is None:
                break
            value, i = got
        elif wire == 2:
            got = read_varint(buf, i)
            if got is None:
                break
            length, i = got
            if i + length > end:
                break
            value = buf[i:i + length]
            i += length
        elif wire == 5:
            value = buf[i:i + 4]
            i += 4
        elif wire == 1:
            value = buf[i:i + 8]
            i += 8
        else:
            break
        out.setdefault(field, []).append(value)
    return out


def _name_pattern(name: bytes) -> bytes:
    length = len(name)
    if length < 128:
        return bytes([0x0A, length]) + name
    return bytes([0x0A, (length & 0x7F) | 0x80, length >> 7]) + name


def find_descriptors(blob: bytes, names: list[str]) -> dict[str, dict[int, list]]:
    """在二进制中定位并解析描述符。解析结果必须自证（name 匹配）才算命中。"""
    found: dict[str, dict[int, list]] = {}
    for name in names:
        raw_name = name.encode()
        if not raw_name.endswith(b".proto"):
            continue
        pattern = _name_pattern(raw_name)
        start = 0
        while True:
            at = blob.find(pattern, start)
            if at < 0:
                break
            parsed = parse_message(blob, at, min(at + 8192, len(blob)))
            got = parsed.get(1)
            if got and got[0] == raw_name:
                found[name] = parsed
                break
            start = at + 1
    return found


def _text(v) -> str:
    return v.decode("utf-8", "replace") if isinstance(v, bytes) else str(v)


def extract_messages(descriptors: dict[str, dict[int, list]]) -> dict[str, list]:
    messages: dict[str, list] = {}
    for desc in descriptors.values():
        package = _text(desc[2][0]) if desc.get(2) else ""
        for raw in desc.get(4, []):
            md = parse_message(raw, 0, len(raw))
            if not md.get(1):
                continue
            fqn = f"{package}.{_text(md[1][0])}" if package else _text(md[1][0])
            fields = []
            for raw_field in md.get(2, []):
                fd = parse_message(raw_field, 0, len(raw_field))
                if not fd.get(1) or not fd.get(3):
                    continue
                fields.append({
                    "no": fd[3][0],
                    "name": _text(fd[1][0]),
                    "type": fd[5][0] if fd.get(5) else 0,
                    "type_name": _text(fd[6][0]).lstrip(".") if fd.get(6) else None,
                })
            messages[fqn] = fields
    return messages


def extract_services(descriptors: dict[str, dict[int, list]]) -> dict[str, dict]:
    services: dict[str, dict] = {}
    for desc in descriptors.values():
        package = _text(desc[2][0]) if desc.get(2) else ""
        for raw in desc.get(6, []):
            sd = parse_message(raw, 0, len(raw))
            if not sd.get(1):
                continue
            svc = f"{package}.{_text(sd[1][0])}" if package else _text(sd[1][0])
            methods = {}
            for raw_m in sd.get(2, []):
                md = parse_message(raw_m, 0, len(raw_m))
                if not md.get(1):
                    continue
                methods[_text(md[1][0])] = {
                    "in": _text(md[2][0]).lstrip(".") if md.get(2) else "",
                    "out": _text(md[3][0]).lstrip(".") if md.get(3) else "",
                    "server_streaming": bool(md.get(6, [0])[0]),
                }
            services[svc] = methods
    return services


def build_schema(messages, enums, services, methods) -> dict:
    return {
        "messages": messages,
        "enums": enums,
        "services": services,
        "methods": methods,
    }


REMOTE_SCRIPT = r"""
import json, re, sys, glob, os
sys.path.insert(0, os.environ["TOOLS_DIR"])
from extract_schema import find_descriptors, extract_messages, extract_services, PROTO_RE
paths = []
for root in ("/ugreen/@appstore", "/volume1/@appstore"):
    for pat in ("*/sbin/*", "*/bin/*", "*/cli/bin/*"):
        paths += glob.glob(os.path.join(root, pat))
blobs = {}
for p in paths:
    try:
        if os.path.isfile(p) and os.path.getsize(p) > 200000:
            blobs[p] = open(p, "rb").read()
    except Exception:
        pass
names = set()
for b in blobs.values():
    for m in PROTO_RE.findall(b):
        names.add(m.decode("utf-8", "replace"))
names = sorted(n for n in names if not n.startswith("google/"))
messages, services = {}, {}
for p, b in blobs.items():
    d = find_descriptors(b, names)
    messages.update(extract_messages(d))
    services.update(extract_services(d))
print(json.dumps({"messages": messages, "services": services}))
"""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", required=True, help="NAS 地址（SSH 可达）")
    ap.add_argument("--ssh-user", default=None)
    ap.add_argument("--out", default="spec/schema.json")
    args = ap.parse_args()

    tools_dir = str(Path(__file__).resolve().parent)
    target = f"{args.ssh_user}@{args.host}" if args.ssh_user else args.host
    proc = subprocess.run(
        ["ssh", "-o", "BatchMode=yes", target,
         f"TOOLS_DIR={tools_dir} python3 -"],
        input=REMOTE_SCRIPT.encode(),
        capture_output=True,
    )
    if proc.returncode != 0:
        print(proc.stderr.decode("utf-8", "replace"), file=sys.stderr)
        return 1

    remote = json.loads(proc.stdout.decode("utf-8"))
    methods = collect_methods(args.host, args.ssh_user)
    schema = build_schema(remote["messages"], {}, remote["services"], methods)

    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(schema, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"wrote {out} ({len(schema['messages'])} messages, "
          f"{len(schema['services'])} services, {len(schema['methods'])} methods)")
    return 0


def collect_methods(host: str, ssh_user: str | None) -> dict:
    """从各应用的 openapi.json 收集 gRPC 方法及其读写属性。"""
    target = f"{ssh_user}@{host}" if ssh_user else host
    script = (
        "import json,glob\n"
        "out={}\n"
        "for f in glob.glob('/ugreen/@appstore/*/openapi.json')"
        " + glob.glob('/volume1/@appstore/*/openapi.json'):\n"
        "    d=json.load(open(f))\n"
        "    sid=(d.get('socket') or {}).get('id','')\n"
        "    for p in d.get('permissions',[]):\n"
        "        for m in p.get('methods',[]):\n"
        "            out[m]={'socket':sid,'operation':p.get('operation',''),"
        "'role':p.get('role','')}\n"
        "print(json.dumps(out))\n"
    )
    proc = subprocess.run(
        ["ssh", "-o", "BatchMode=yes", target, "python3 -"],
        input=script.encode(), capture_output=True,
    )
    if proc.returncode != 0:
        raise SystemExit("无法读取 openapi.json：" + proc.stderr.decode("utf-8", "replace"))
    return json.loads(proc.stdout.decode("utf-8"))


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 4: 运行测试确认通过**

```bash
cd tools && python -m pytest tests/test_extract.py -v
```
Expected: 4 passed

- [ ] **Step 5: 对真机跑一次，确认产出**

```bash
python tools/extract_schema.py --host <你的 NAS 地址>
python -c "import json;d=json.load(open('spec/schema.json'));print(len(d['messages']),len(d['services']),len(d['methods']))"
```
Expected: 三个数字均 > 0，且 `methods` 为 101

- [ ] **Step 6: 提交**

```bash
git add tools/extract_schema.py tools/tests/test_extract.py
git commit -m "feat(tools): extract protobuf descriptors from vendor binaries"
```

---

### Task 7: 命令表 + `list` + `call`

**Files:**
- Create: `src/commands/mod.rs`
- Create: `src/main.rs`（替换）
- Modify: `src/lib.rs`
- Test: `tests/commands.rs`

**Interfaces:**
- Consumes: `schema::Schema`、`client::Client`、`wire::Message`
- Produces:
  - `pub struct Ctx { pub schema: Schema, pub client: Client }`
  - `pub fn emit_ok(method: &str, data: serde_json::Value)`
  - `pub fn emit_err(e: &ApiError) -> i32`
  - `pub fn parse_field(spec: &str) -> Result<(u32, Value), String>`，接受 `3:string=/path`、`8:int=10`、`9:bool=true`
  - `pub fn require_yes(operation: &str, method: &str, detail: &str, yes: bool) -> Result<(), String>`

- [ ] **Step 1: 写失败测试 `tests/commands.rs`**

```rust
use nascli::commands::{parse_field, require_yes};
use nascli::wire::Value;

#[test]
fn parses_string_field() {
    let (no, v) = parse_field("3:string=/volume1/demo").unwrap();
    assert_eq!(no, 3);
    assert_eq!(v, Value::Bytes(b"/volume1/demo".to_vec()));
}

#[test]
fn parses_int_field() {
    let (no, v) = parse_field("8:int=50").unwrap();
    assert_eq!(no, 8);
    assert_eq!(v, Value::Varint(50));
}

#[test]
fn parses_bool_field() {
    let (_, v) = parse_field("9:bool=true").unwrap();
    assert_eq!(v, Value::Varint(1));
}

#[test]
fn rejects_bad_field_spec() {
    assert!(parse_field("3").is_err());
    assert!(parse_field("x:string=a").is_err());
    assert!(parse_field("3:unknown=a").is_err());
    assert!(parse_field("8:int=abc").is_err());
}

#[test]
fn read_operation_needs_no_confirmation() {
    assert!(require_yes("read", "/a/B", "", false).is_ok());
}

#[test]
fn write_operation_requires_yes() {
    let e = require_yes("delete", "/a/B", "删除 /volume1/demo", false).unwrap_err();
    assert!(e.contains("--yes"));
    assert!(require_yes("delete", "/a/B", "删除 /volume1/demo", true).is_ok());
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test --test commands
```
Expected: 编译失败，`unresolved import nascli::commands`

- [ ] **Step 3: 实现 `src/commands/mod.rs`**

```rust
// 各命令模块由后续任务逐个加入（Task 8 起），本任务不声明它们，否则无法编译：
//   Task 8: pub mod storage;   Task 9: pub mod system;
//   Task 10: pub mod docker;   Task 11: pub mod apps; pub mod users; pub mod download;
//   Task 12: pub mod files;

use crate::client::{ApiError, Client};
use crate::schema::{is_read_only, Schema};
use crate::wire::Value;
use serde_json::json;

pub struct Ctx {
    pub client: Client,
}

pub fn emit_ok(method: &str, data: serde_json::Value) {
    println!("{}", json!({"ok": true, "method": method, "data": data}));
}

pub fn emit_err(e: &ApiError) -> i32 {
    println!(
        "{}",
        json!({"ok": false, "error": {
            "code": e.code(),
            "message": e.message(),
            "exit_code": e.exit_code()
        }})
    );
    e.exit_code()
}

/// 解析 `--field <no>:<type>=<value>`。
pub fn parse_field(spec: &str) -> Result<(u32, Value), String> {
    let (head, value) = spec
        .split_once('=')
        .ok_or_else(|| format!("字段格式应为 <编号>:<类型>=<值>，收到：{spec}"))?;
    let (no, kind) = head
        .split_once(':')
        .ok_or_else(|| format!("字段格式应为 <编号>:<类型>=<值>，收到：{spec}"))?;
    let no: u32 = no.parse().map_err(|_| format!("字段编号不是数字：{no}"))?;
    let v = match kind {
        "string" => Value::Bytes(value.as_bytes().to_vec()),
        "int" => Value::Varint(
            value.parse::<u64>().map_err(|_| format!("不是合法整数：{value}"))?,
        ),
        "bool" => Value::Varint(match value {
            "true" | "1" => 1,
            "false" | "0" => 0,
            other => return Err(format!("不是合法布尔值：{other}")),
        }),
        other => return Err(format!("不支持的类型：{other}（可用 string/int/bool）")),
    };
    Ok((no, v))
}

/// 写类操作的确认闸门。
pub fn require_yes(
    operation: &str,
    method: &str,
    detail: &str,
    yes: bool,
) -> Result<(), String> {
    if is_read_only(operation) || yes {
        return Ok(());
    }
    let mut msg = format!(
        "该操作会修改 NAS 状态，已被拒绝\n  method:  {method}   [{operation}]"
    );
    if !detail.is_empty() {
        msg.push_str(&format!("\n  将执行:  {detail}"));
    }
    msg.push_str("\n  如确认执行，请重试并加上 --yes");
    Err(msg)
}
```

- [ ] **Step 4: 实现 `src/main.rs`（含 `list` 与 `call`）**

```rust
use clap::{Parser, Subcommand};
use nascli::client::Client;
use nascli::commands::{emit_err, emit_ok, parse_field, require_yes, Ctx};
use nascli::config::Config;
use nascli::schema::{is_read_only, Schema};
use nascli::wire::Message;
use std::collections::BTreeMap;
use std::process::ExitCode;

const SCHEMA_JSON: &str = include_str!("../spec/schema.json");

#[derive(Parser)]
#[command(name = "nascli", version, about = "UGOS Pro OpenCLI 客户端")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 列出所有可用方法
    List {
        #[arg(long)]
        module: Option<String>,
        #[arg(long)]
        read_only: bool,
    },
    /// 通用调用（兜底）
    Call {
        socket: String,
        method: String,
        #[arg(long = "field", value_name = "N:TYPE=V")]
        fields: Vec<String>,
        #[arg(long)]
        yes: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let schema = match Schema::from_json_str(SCHEMA_JSON) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("内置 schema 解析失败：{e}");
            return ExitCode::from(7);
        }
    };

    match cli.command {
        Command::List { module, read_only } => {
            let mut rows = vec![];
            for (path, info) in schema.methods() {
                if read_only && !is_read_only(&info.operation) {
                    continue;
                }
                if let Some(m) = &module {
                    if !info.socket.contains(m.as_str()) && !path.contains(m.as_str()) {
                        continue;
                    }
                }
                rows.push(serde_json::json!({
                    "method": path,
                    "socket": info.socket,
                    "operation": info.operation,
                    "role": info.role,
                }));
            }
            emit_ok("list", serde_json::Value::Array(rows));
            ExitCode::SUCCESS
        }
        Command::Call { socket, method, fields, yes } => {
            let cfg = match Config::from_env() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
            };
            let client = match Client::new(cfg) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(6);
                }
            };
            let info = schema.method(&method);
            let operation = info.map(|i| i.operation.as_str()).unwrap_or("read");
            let detail = fields.join(" ");
            if let Err(e) = require_yes(operation, &method, &detail, yes) {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
            let mut msg: Message = BTreeMap::new();
            for spec in &fields {
                match parse_field(spec) {
                    Ok((no, v)) => {
                        msg.insert(no, v);
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        return ExitCode::from(2);
                    }
                }
            }
            match client.call(&socket, &method, &msg) {
                Ok(resp) => {
                    emit_ok(&method, nascli::schema::to_json(&resp, &schema));
                    ExitCode::SUCCESS
                }
                Err(e) => ExitCode::from(emit_err(&e) as u8),
            }
        }
    }
}
```

- [ ] **Step 5: 运行测试确认通过**

```bash
cargo test --test commands
cargo build
```
Expected: 6 passed，编译成功

- [ ] **Step 6: 提交**

```bash
git add src/commands/mod.rs src/main.rs src/lib.rs tests/commands.rs
git commit -m "feat(cli): command registry with list and generic call"
```

---

### Task 8: `storage` 命令

**Files:**
- Create: `src/commands/storage.rs`
- Modify: `src/main.rs`（挂载子命令）
- Test: `tests/storage_cmd.rs`

**Interfaces:**
- Consumes: `commands::Ctx`
- Produces: `pub fn run(ctx: &Ctx, action: &str) -> Result<serde_json::Value, ApiError>`
  - `action` ∈ `disks | volumes | pools | smart`

- [ ] **Step 1: 写失败测试 `tests/storage_cmd.rs`**

```rust
use nascli::commands::storage::resolve;

#[test]
fn maps_actions_to_methods() {
    assert_eq!(resolve("disks").unwrap(),
               ("ugreen.storage.grpc", "/storage.Storage/DiskList"));
    assert_eq!(resolve("volumes").unwrap(),
               ("ugreen.storage.grpc", "/storage.Storage/VolumeList"));
    assert_eq!(resolve("pools").unwrap(),
               ("ugreen.storage.grpc", "/storage.Storage/PoolList"));
    assert_eq!(resolve("smart").unwrap(),
               ("ugreen.storage.grpc", "/storage.Storage/AllDiskSmartInfo"));
}

#[test]
fn rejects_unknown_action() {
    assert!(resolve("nope").is_err());
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test --test storage_cmd
```
Expected: 编译失败，`unresolved import nascli::commands::storage`

- [ ] **Step 3: 实现 `src/commands/storage.rs`**

```rust
use crate::client::{ApiError, Client};
use crate::wire::Message;
use std::collections::BTreeMap;

pub const SOCKET: &str = "ugreen.storage.grpc";

pub fn resolve(action: &str) -> Result<(&'static str, &'static str), String> {
    let method = match action {
        "disks" => "/storage.Storage/DiskList",
        "volumes" => "/storage.Storage/VolumeList",
        "pools" => "/storage.Storage/PoolList",
        "smart" => "/storage.Storage/AllDiskSmartInfo",
        other => {
            return Err(format!(
                "未知的 storage 子命令：{other}（可用：disks, volumes, pools, smart）"
            ))
        }
    };
    Ok((SOCKET, method))
}

pub fn run(client: &Client, action: &str) -> Result<Message, ApiError> {
    let (socket, method) = resolve(action).map_err(ApiError::BadRequest)?;
    client.call(socket, method, &BTreeMap::new())
}
```

- [ ] **Step 4: 在 `src/main.rs` 挂载子命令**

在 `enum Command` 中新增：

```rust
    /// 存储管理
    Storage {
        #[arg(value_name = "ACTION")]
        action: String,
    },
```

在 `match cli.command` 中新增分支（与 `Call` 分支共用配置与客户端的构造，抽成 `fn build_ctx() -> Result<Ctx, ExitCode>`）：

```rust
        Command::Storage { action } => {
            let ctx = match build_ctx() {
                Ok(c) => c,
                Err(code) => return code,
            };
            let (socket, method) = match nascli::commands::storage::resolve(&action) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
            };
            match ctx.client.call(socket, method, &BTreeMap::new()) {
                Ok(resp) => {
                    emit_ok(method, nascli::schema::to_json(&resp, &schema));
                    ExitCode::SUCCESS
                }
                Err(e) => ExitCode::from(emit_err(&e) as u8),
            }
        }
```

`build_ctx` 实现：

```rust
fn build_ctx() -> Result<Ctx, ExitCode> {
    let cfg = Config::from_env().map_err(|e| {
        eprintln!("{e}");
        ExitCode::from(2)
    })?;
    let client = Client::new(cfg).map_err(|e| {
        eprintln!("{e}");
        ExitCode::from(6)
    })?;
    Ok(Ctx { client })
}
```

- [ ] **Step 5: 运行测试确认通过**

```bash
cargo test --test storage_cmd && cargo build
```
Expected: 2 passed

- [ ] **Step 6: 对真机验证（需环境变量）**

```bash
UGNAS_HOST=<NAS> UGPOA_TOKEN=<token> cargo run -- storage volumes
```
Expected: 输出含 `"ok": true` 与卷信息

- [ ] **Step 7: 提交**

```bash
git add src/commands/storage.rs src/main.rs tests/storage_cmd.rs
git commit -m "feat(storage): disks/volumes/pools/smart commands"
```

---

### Task 9: `system` 与 `network` 命令

**Files:**
- Create: `src/commands/system.rs`
- Modify: `src/main.rs`
- Test: `tests/system_cmd.rs`

**Interfaces:**
- Consumes: `client::Client`
- Produces: `pub fn resolve(action: &str) -> Result<(&'static str, &'static str), String>`
  - system：`identity | update-status | cpu | mem | net | disk | volume | overview | services | processes | ups | power | fan | led | beep | cpu-config | zram`
  - network：`general | ifaces`

- [ ] **Step 1: 写失败测试 `tests/system_cmd.rs`**

```rust
use nascli::commands::system::{resolve_network, resolve_system};

#[test]
fn maps_system_actions() {
    assert_eq!(resolve_system("cpu").unwrap(),
               ("ugreen.taskmgr.internal.grpc", "/taskserv.TaskManage/GetCpuMetric"));
    assert_eq!(resolve_system("identity").unwrap(),
               ("ugreen.ctlserv.internal.grpc", "/ctlserv.Internal/GetOsIdentity"));
    assert_eq!(resolve_system("ups").unwrap(),
               ("ugreen.ctlserv.internal.grpc", "/ctlserv.Internal/GetUpsConfig"));
    assert_eq!(resolve_system("fan").unwrap(),
               ("ugreen.ctlserv.internal.grpc", "/ctlserv.Hardware/GetFanConfig"));
}

#[test]
fn maps_network_actions() {
    assert_eq!(resolve_network("general").unwrap(),
               ("ugreen.ctlserv.internal.grpc", "/ctlserv.Network/GetGeneralConfig"));
    assert_eq!(resolve_network("ifaces").unwrap(),
               ("ugreen.ctlserv.internal.grpc", "/ctlserv.Network/GetIfaceList"));
}

#[test]
fn rejects_unknown() {
    assert!(resolve_system("nope").is_err());
    assert!(resolve_network("nope").is_err());
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test --test system_cmd
```
Expected: 编译失败，`unresolved import nascli::commands::system`

- [ ] **Step 3: 实现 `src/commands/system.rs`**

```rust
use crate::client::{ApiError, Client};
use crate::wire::Message;
use std::collections::BTreeMap;

const TASK: &str = "ugreen.taskmgr.internal.grpc";
const CTL: &str = "ugreen.ctlserv.internal.grpc";

pub fn resolve_system(action: &str) -> Result<(&'static str, &'static str), String> {
    let pair = match action {
        "identity" => (CTL, "/ctlserv.Internal/GetOsIdentity"),
        "update-status" => (CTL, "/ctlserv.Internal/GetOsUpdateStatus"),
        "cpu" => (TASK, "/taskserv.TaskManage/GetCpuMetric"),
        "mem" => (TASK, "/taskserv.TaskManage/GetMemMetric"),
        "net" => (TASK, "/taskserv.TaskManage/GetNetTraffic"),
        "disk" => (TASK, "/taskserv.TaskManage/GetDiskMetric"),
        "volume" => (TASK, "/taskserv.TaskManage/GetVolumeMetric"),
        "overview" => (TASK, "/taskserv.TaskManage/GetRunOverview"),
        "services" => (TASK, "/taskserv.TaskManage/GetServiceResource"),
        "processes" => (TASK, "/taskserv.TaskManage/GetProcessResource"),
        "ups" => (CTL, "/ctlserv.Internal/GetUpsConfig"),
        "power" => (CTL, "/ctlserv.Hardware/GetPowerConfig"),
        "fan" => (CTL, "/ctlserv.Hardware/GetFanConfig"),
        "led" => (CTL, "/ctlserv.Hardware/GetLedConfig"),
        "beep" => (CTL, "/ctlserv.Hardware/GetBeepConfig"),
        "cpu-config" => (CTL, "/ctlserv.Hardware/GetCpuConfig"),
        "zram" => (CTL, "/ctlserv.Hardware/GetZramConfig"),
        other => return Err(format!("未知的 system 子命令：{other}")),
    };
    Ok(pair)
}

pub fn resolve_network(action: &str) -> Result<(&'static str, &'static str), String> {
    let pair = match action {
        "general" => (CTL, "/ctlserv.Network/GetGeneralConfig"),
        "ifaces" => (CTL, "/ctlserv.Network/GetIfaceList"),
        other => return Err(format!("未知的 network 子命令：{other}")),
    };
    Ok(pair)
}

pub fn run(client: &Client, socket: &str, method: &str) -> Result<Message, ApiError> {
    client.call(socket, method, &BTreeMap::new())
}
```

- [ ] **Step 4: 在 `src/main.rs` 挂载**

`enum Command` 新增：

```rust
    /// 系统信息与指标
    System { action: String },
    /// 网络配置
    Network { action: String },
```

`match` 分支：

```rust
        Command::System { action } => {
            let ctx = match build_ctx() { Ok(c) => c, Err(code) => return code };
            match nascli::commands::system::resolve_system(&action) {
                Ok((socket, method)) => {
                    match ctx.client.call(socket, method, &BTreeMap::new()) {
                        Ok(resp) => { emit_ok(method, nascli::schema::to_json(&resp, &schema)); ExitCode::SUCCESS }
                        Err(e) => ExitCode::from(emit_err(&e) as u8),
                    }
                }
                Err(e) => { eprintln!("{e}"); ExitCode::from(2) }
            }
        }
        Command::Network { action } => {
            let ctx = match build_ctx() { Ok(c) => c, Err(code) => return code };
            match nascli::commands::system::resolve_network(&action) {
                Ok((socket, method)) => {
                    match ctx.client.call(socket, method, &BTreeMap::new()) {
                        Ok(resp) => { emit_ok(method, nascli::schema::to_json(&resp, &schema)); ExitCode::SUCCESS }
                        Err(e) => ExitCode::from(emit_err(&e) as u8),
                    }
                }
                Err(e) => { eprintln!("{e}"); ExitCode::from(2) }
            }
        }
```

- [ ] **Step 5: 运行测试确认通过**

```bash
cargo test --test system_cmd && cargo build
```
Expected: 3 passed

- [ ] **Step 6: 提交**

```bash
git add src/commands/system.rs src/main.rs tests/system_cmd.rs
git commit -m "feat(system): system metrics and network commands"
```

---

### Task 10: `docker` 命令

**Files:**
- Create: `src/commands/docker.rs`
- Modify: `src/main.rs`
- Test: `tests/docker_cmd.rs`

**Interfaces:**
- Consumes: `client::Client`
- Produces:
  - `pub fn resolve(action: &str) -> Result<(&'static str, &'static str, &'static str), String>`（socket, method, operation）
  - 只读：`containers | images | compose | logs | port-check | dir-check | usage`
  - 写类：`start | stop | restart | pull | compose-start | compose-stop | compose-restart | compose-delete | rebuild`

- [ ] **Step 1: 写失败测试 `tests/docker_cmd.rs`**

```rust
use nascli::commands::docker::resolve;

#[test]
fn maps_read_actions() {
    assert_eq!(resolve("containers").unwrap(),
               ("ugreen.dockerserv.internal.grpc", "/docker.Docker/ContainerList", "read"));
    assert_eq!(resolve("images").unwrap(),
               ("ugreen.dockerserv.internal.grpc", "/docker.Docker/ShowLocalImage", "read"));
    assert_eq!(resolve("compose").unwrap(),
               ("ugreen.dockerserv.internal.grpc", "/docker.Docker/GetComposeProjectList", "read"));
}

#[test]
fn maps_write_actions_with_operation() {
    let (_, method, op) = resolve("stop").unwrap();
    assert_eq!(method, "/docker.Docker/StopContainer");
    assert_eq!(op, "execute");
    let (_, _, op) = resolve("compose-delete").unwrap();
    assert_eq!(op, "delete");
}

#[test]
fn rejects_unknown_action() {
    assert!(resolve("nope").is_err());
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test --test docker_cmd
```
Expected: 编译失败，`unresolved import nascli::commands::docker`

- [ ] **Step 3: 实现 `src/commands/docker.rs`**

```rust
use crate::client::{ApiError, Client};
use crate::wire::Message;
use std::collections::BTreeMap;

pub const SOCKET: &str = "ugreen.dockerserv.internal.grpc";

pub fn resolve(action: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    let (method, op) = match action {
        "containers" => ("/docker.Docker/ContainerList", "read"),
        "images" => ("/docker.Docker/ShowLocalImage", "read"),
        "compose" => ("/docker.Docker/GetComposeProjectList", "read"),
        "logs" => ("/docker.Docker/GetContainerLogs", "read"),
        "port-check" => ("/docker.Docker/CheckPortOccupiedByContainer", "read"),
        "dir-check" => ("/docker.Docker/CheckDirUsedByContainer", "read"),
        "usage" => ("/docker.Docker/GetAppUsage", "read"),
        "pull" => ("/docker.Docker/PullImage", "write"),
        "start" => ("/docker.Docker/StartContainer", "execute"),
        "stop" => ("/docker.Docker/StopContainer", "execute"),
        "restart" => ("/docker.Docker/RestartContainer", "execute"),
        "compose-start" => ("/docker.Docker/StartComposeProject", "execute"),
        "compose-stop" => ("/docker.Docker/StopComposeProject", "execute"),
        "compose-restart" => ("/docker.Docker/RestartComposeProject", "execute"),
        "compose-delete" => ("/docker.Docker/DeleteComposeProject", "delete"),
        "rebuild" => ("/docker.Docker/RebuildContainerApp", "execute"),
        other => return Err(format!("未知的 docker 子命令：{other}")),
    };
    Ok((SOCKET, method, op))
}

pub fn run(client: &Client, socket: &str, method: &str, msg: &Message) -> Result<Message, ApiError> {
    client.call(socket, method, msg)
}

pub fn empty() -> Message {
    BTreeMap::new()
}
```

- [ ] **Step 4: 在 `src/main.rs` 挂载**

`enum Command` 新增：

```rust
    /// Docker 管理
    Docker {
        action: String,
        /// 写类操作必须显式确认
        #[arg(long)]
        yes: bool,
    },
```

`match` 分支：

```rust
        Command::Docker { action, yes } => {
            let (socket, method, op) = match nascli::commands::docker::resolve(&action) {
                Ok(v) => v,
                Err(e) => { eprintln!("{e}"); return ExitCode::from(2); }
            };
            if let Err(e) = require_yes(op, method, &action, yes) {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
            let ctx = match build_ctx() { Ok(c) => c, Err(code) => return code };
            match ctx.client.call(socket, method, &BTreeMap::new()) {
                Ok(resp) => { emit_ok(method, nascli::schema::to_json(&resp, &schema)); ExitCode::SUCCESS }
                Err(e) => ExitCode::from(emit_err(&e) as u8),
            }
        }
```

- [ ] **Step 5: 运行测试确认通过**

```bash
cargo test --test docker_cmd && cargo build
```
Expected: 3 passed

- [ ] **Step 6: 验证写类闸门**

```bash
UGNAS_HOST=<NAS> UGPOA_TOKEN=<token> cargo run -- docker stop; echo "exit=$?"
```
Expected: 拒绝执行，`exit=2`，stderr 含 `--yes`

- [ ] **Step 7: 提交**

```bash
git add src/commands/docker.rs src/main.rs tests/docker_cmd.rs
git commit -m "feat(docker): container and compose commands with write gate"
```

---

### Task 11: `apps` / `users` / `download` 命令

**Files:**
- Create: `src/commands/apps.rs`
- Create: `src/commands/users.rs`
- Create: `src/commands/download.rs`
- Modify: `src/main.rs`
- Test: `tests/misc_cmd.rs`

**Interfaces:**
- Consumes: `client::Client`
- Produces:
  - `apps::resolve(action) -> Result<(&str, &str, &str), String>`：`list | detail | status`
  - `users::resolve(action) -> Result<(&str, &str), String>`：`list | groups`
  - `download::resolve(action) -> Result<(&str, &str, &str), String>`：`tasks`

- [ ] **Step 1: 写失败测试 `tests/misc_cmd.rs`**

```rust
use nascli::commands::{apps, download, users};

#[test]
fn maps_apps() {
    assert_eq!(apps::resolve("list").unwrap(),
               ("ugreen.app.grpc", "/app.App/DetailList", "read"));
    assert_eq!(apps::resolve("detail").unwrap(),
               ("ugreen.app.grpc", "/app.App/GetAppDetail", "read"));
    assert_eq!(apps::resolve("status").unwrap(),
               ("ugreen.app.grpc", "/app.App/GetAppStatus", "read"));
}

#[test]
fn maps_users() {
    assert_eq!(users::resolve("list").unwrap(),
               ("ugreen.ctlserv.internal.grpc", "/ctlserv.Internal/GetUgreenUserList"));
    assert_eq!(users::resolve("groups").unwrap(),
               ("ugreen.ctlserv.internal.grpc", "/ctlserv.Internal/GetCreatedGroupList"));
}

#[test]
fn maps_download() {
    assert_eq!(download::resolve("tasks").unwrap(),
               ("ugreen.downloadserv.internal.grpc",
                "/download.DownloadService/BatchGetDownloadTask", "read"));
}

#[test]
fn rejects_unknown() {
    assert!(apps::resolve("nope").is_err());
    assert!(users::resolve("nope").is_err());
    assert!(download::resolve("nope").is_err());
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test --test misc_cmd
```
Expected: 编译失败

- [ ] **Step 3: 实现三个模块**

`src/commands/apps.rs`:
```rust
pub const SOCKET: &str = "ugreen.app.grpc";

pub fn resolve(action: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    let (method, op) = match action {
        "list" => ("/app.App/DetailList", "read"),
        "detail" => ("/app.App/GetAppDetail", "read"),
        "status" => ("/app.App/GetAppStatus", "read"),
        other => return Err(format!("未知的 apps 子命令：{other}（可用：list, detail, status）")),
    };
    Ok((SOCKET, method, op))
}
```

`src/commands/users.rs`:
```rust
pub const SOCKET: &str = "ugreen.ctlserv.internal.grpc";

pub fn resolve(action: &str) -> Result<(&'static str, &'static str), String> {
    let method = match action {
        "list" => "/ctlserv.Internal/GetUgreenUserList",
        "groups" => "/ctlserv.Internal/GetCreatedGroupList",
        other => return Err(format!("未知的 users 子命令：{other}（可用：list, groups）")),
    };
    Ok((SOCKET, method))
}
```

`src/commands/download.rs`:
```rust
pub const SOCKET: &str = "ugreen.downloadserv.internal.grpc";

pub fn resolve(action: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    let (method, op) = match action {
        "tasks" => ("/download.DownloadService/BatchGetDownloadTask", "read"),
        other => return Err(format!("未知的 download 子命令：{other}（可用：tasks）")),
    };
    Ok((SOCKET, method, op))
}
```

- [ ] **Step 4: 在 `src/main.rs` 挂载三个子命令**

`enum Command` 新增：

```rust
    /// 应用中心
    Apps { action: String },
    /// 用户与用户组
    Users { action: String },
    /// 下载中心
    Download { action: String },
```

`match` 分支（三者结构一致，以 `Apps` 为例）：

```rust
        Command::Apps { action } => {
            let (socket, method, _op) = match nascli::commands::apps::resolve(&action) {
                Ok(v) => v,
                Err(e) => { eprintln!("{e}"); return ExitCode::from(2); }
            };
            let ctx = match build_ctx() { Ok(c) => c, Err(code) => return code };
            match ctx.client.call(socket, method, &BTreeMap::new()) {
                Ok(resp) => { emit_ok(method, nascli::schema::to_json(&resp, &schema)); ExitCode::SUCCESS }
                Err(e) => ExitCode::from(emit_err(&e) as u8),
            }
        }
```

`Users` 与 `Download` 同构，分别调用 `users::resolve` / `download::resolve`。

- [ ] **Step 5: 运行测试确认通过**

```bash
cargo test --test misc_cmd && cargo build
```
Expected: 4 passed

- [ ] **Step 6: 提交**

```bash
git add src/commands/apps.rs src/commands/users.rs src/commands/download.rs src/main.rs tests/misc_cmd.rs
git commit -m "feat(commands): apps, users and download modules"
```

---

### Task 12: `fs` 命令

**Files:**
- Create: `src/commands/files.rs`
- Modify: `src/main.rs`
- Test: `tests/files_cmd.rs`

**Interfaces:**
- Consumes: `client::Client`、`wire::{Message, Value}`
- Produces:
  - `pub fn resolve(action: &str) -> Result<(&'static str, &'static str, &'static str), String>`
  - `pub fn build_message(action: &str, path: &str, page: Option<u64>, limit: Option<u64>) -> Result<Message, String>`

**关键点：**
- `ls` 用**一元**的 `CategoryOrFileDirList`（`path` 在字段 3），**不用** `Readdir`（流式，不支持）。
- `du` 直接返回错误，说明流式不支持。

- [ ] **Step 1: 写失败测试 `tests/files_cmd.rs`**

```rust
use nascli::commands::files::{build_message, resolve};
use nascli::schema::Schema;
use nascli::wire::Value;

const SAMPLE: &str = r#"{
  "messages": {
    "ugidl.filesystem.cli.FileDirListRequest": [
      {"no": 3, "name": "path", "type": "string"},
      {"no": 7, "name": "page", "type": "int32"},
      {"no": 8, "name": "limit", "type": "int32"}
    ],
    "ugidl.filesystem.cli.StatRequest": [
      {"no": 1, "name": "path", "type": "string"}
    ]
  },
  "enums": {},
  "services": {
    "ugidl.filesystem.Filesystem": {
      "CategoryOrFileDirList": {"in": "ugidl.filesystem.cli.FileDirListRequest", "out": "x", "server_streaming": false},
      "Stat": {"in": "ugidl.filesystem.cli.StatRequest", "out": "x", "server_streaming": false}
    }
  },
  "methods": {}
}"#;

fn schema() -> Schema {
    Schema::from_json_str(SAMPLE).unwrap()
}

#[test]
fn ls_uses_unary_file_dir_list() {
    let (_, method, op) = resolve("ls").unwrap();
    assert_eq!(method, "/ugidl.filesystem.Filesystem/CategoryOrFileDirList");
    assert_eq!(op, "read");
}

#[test]
fn ls_resolves_path_field_from_schema() {
    let s = schema();
    let (_, method, _) = resolve("ls").unwrap();
    let m = build_message(&s, method, "ls", "/volume1/demo", Some(1), Some(50)).unwrap();
    assert_eq!(m.get(&3), Some(&Value::Bytes(b"/volume1/demo".to_vec())));
    assert_eq!(m.get(&7), Some(&Value::Varint(1)));
    assert_eq!(m.get(&8), Some(&Value::Varint(50)));
}

#[test]
fn stat_resolves_path_field_from_schema() {
    let s = schema();
    let (_, method, _) = resolve("stat").unwrap();
    let m = build_message(&s, method, "stat", "/volume1/demo", None, None).unwrap();
    assert_eq!(m.get(&1), Some(&Value::Bytes(b"/volume1/demo".to_vec())));
}

#[test]
fn multi_param_action_is_rejected() {
    let s = schema();
    let (_, method, _) = resolve("rename").unwrap();
    let e = build_message(&s, method, "rename", "/volume1/demo", None, None).unwrap_err();
    assert!(e.contains("nascli call"));
}

#[test]
fn du_is_reported_unsupported() {
    let e = resolve("du").unwrap_err();
    assert!(e.contains("流式"));
}

#[test]
fn delete_requires_yes_operation() {
    let (_, method, op) = resolve("del").unwrap();
    assert_eq!(method, "/ugidl.filesystem.Filesystem/DelPath");
    assert_eq!(op, "delete");
}

#[test]
fn rejects_unknown_action() {
    assert!(resolve("nope").is_err());
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test --test files_cmd
```
Expected: 编译失败

- [ ] **Step 3: 实现 `src/commands/files.rs`**

```rust
use crate::schema::Schema;
use crate::wire::{Message, Value};
use std::collections::BTreeMap;

pub const SOCKET: &str = "ugreen.filemgr.internal.grpc";

/// 返回 (socket, method, operation)。
/// `du` 与 `find` 因服务端流式而不可用，直接返回错误。
pub fn resolve(action: &str) -> Result<(&'static str, &'static str, &'static str), String> {
    let (method, op) = match action {
        "stat" => (concat!("/ugidl.filesystem.Filesystem", "/Stat"), "read"),
        "ls" => (concat!("/ugidl.filesystem.Filesystem", "/CategoryOrFileDirList"), "read"),
        "cat" => (concat!("/ugidl.filesystem.Filesystem", "/ReadFile"), "read"),
        "getfattr" => (concat!("/ugidl.filesystem.Filesystem", "/Getfattr"), "read"),
        "recycle-status" => (concat!("/ugidl.filesystem.Filesystem", "/GetRecycleStatus"), "read"),
        "path-perm" => (concat!("/ugidl.filesystem.Filesystem", "/GetPathPermission"), "read"),
        "md5sum" => (concat!("/ugidl.filesystem.Filesystem", "/Md5sum"), "read"),
        "sha256sum" => (concat!("/ugidl.filesystem.Filesystem", "/Sha256sum"), "read"),
        "mkdir" => (concat!("/ugidl.filesystem.Filesystem", "/CreateDir"), "write"),
        "touch" => (concat!("/ugidl.filesystem.Filesystem", "/CreateFile"), "write"),
        "echo" => (concat!("/ugidl.filesystem.Filesystem", "/WriteFile"), "write"),
        "rename" => (concat!("/ugidl.filesystem.Filesystem", "/Rename"), "write"),
        "cpmv" => (concat!("/ugidl.filesystem.Filesystem", "/CpMovePath"), "write"),
        "del" => (concat!("/ugidl.filesystem.Filesystem", "/DelPath"), "delete"),
        "restore" => (concat!("/ugidl.filesystem.Filesystem", "/RecoverFromRecycle"), "write"),
        "chtimes" => (concat!("/ugidl.filesystem.Filesystem", "/Chtimes"), "write"),
        "modify-acl" => (concat!("/ugidl.filesystem.Filesystem", "/ModifyPathAcl"), "write"),
        "share-mkdir" => (concat!("/ugidl.filesystem.Filesystem", "/CreateShareFolder"), "write"),
        "du" => {
            return Err(
                "du 对应的服务端方法为流式 RPC，OpenCLI 的 HTTP 接口不支持".to_string()
            )
        }
        "find" => {
            return Err(
                "find 对应的服务端方法为流式 RPC，OpenCLI 的 HTTP 接口不支持".to_string()
            )
        }
        other => return Err(format!("未知的 fs 子命令：{other}")),
    };
    Ok((SOCKET, method, op))
}

/// 按 schema 里的**字段名**构造请求，不硬编码字段号。
/// 多参数动作（rename / cpmv / echo / md5sum 等）不在此处理，引导用户走 `nascli call`。
pub fn build_message(
    schema: &Schema,
    method: &str,
    action: &str,
    path: &str,
    page: Option<u64>,
    limit: Option<u64>,
) -> Result<Message, String> {
    let req_type = schema
        .request_type(method)
        .ok_or_else(|| format!("schema 中找不到 {method} 的请求类型"))?;

    let path_field = match action {
        "stat" | "ls" | "cat" | "getfattr" | "recycle-status" | "path-perm"
        | "mkdir" | "touch" | "del" | "restore" => "path",
        other => {
            return Err(format!(
                "{other} 需要多个参数，请改用 nascli call 显式指定字段"
            ))
        }
    };

    let mut m: Message = BTreeMap::new();
    let no = schema
        .field_no(&req_type, path_field)
        .ok_or_else(|| format!("{req_type} 中没有字段 {path_field}"))?;
    m.insert(no, Value::Bytes(path.as_bytes().to_vec()));

    if action == "ls" {
        for (name, value) in [("page", page), ("limit", limit)] {
            if let Some(v) = value {
                let no = schema
                    .field_no(&req_type, name)
                    .ok_or_else(|| format!("{req_type} 中没有字段 {name}"))?;
                m.insert(no, Value::Varint(v));
            }
        }
    }
    Ok(m)
}
```

> 设计要点：字段号来自 `schema.field_no()`，命令表只写**字段名**。
> 固件升级即使调整了字段号，只要名字不变，CLI 无需改动。

- [ ] **Step 4: 在 `src/main.rs` 挂载 `fs`**

`enum Command` 新增：

```rust
    /// 文件操作
    Fs {
        action: String,
        #[arg(long, default_value = "")]
        path: String,
        #[arg(long)]
        page: Option<u64>,
        #[arg(long)]
        limit: Option<u64>,
        #[arg(long)]
        yes: bool,
    },
```

`match` 分支：

```rust
        Command::Fs { action, path, page, limit, yes } => {
            let (socket, method, op) = match nascli::commands::files::resolve(&action) {
                Ok(v) => v,
                Err(e) => { eprintln!("{e}"); return ExitCode::from(5); }
            };
            if let Err(e) = require_yes(op, method, &path, yes) {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
            let msg = match nascli::commands::files::build_message(
                &schema, method, &action, &path, page, limit,
            ) {
                Ok(m) => m,
                Err(e) => { eprintln!("{e}"); return ExitCode::from(2); }
            };
            let ctx = match build_ctx() { Ok(c) => c, Err(code) => return code };
            match ctx.client.call(socket, method, &msg) {
                Ok(resp) => { emit_ok(method, nascli::schema::to_json(&resp, &schema)); ExitCode::SUCCESS }
                Err(e) => ExitCode::from(emit_err(&e) as u8),
            }
        }
```

- [ ] **Step 5: 运行测试确认通过**

```bash
cargo test --test files_cmd && cargo build
```
Expected: 7 passed

- [ ] **Step 6: 对真机验证 `fs ls`**

```bash
UGNAS_HOST=<NAS> UGPOA_TOKEN=<token> cargo run -- fs ls --path /volume1/demo --limit 5
```
Expected: 输出目录项 JSON。若报「没有字段 path」，说明该请求消息的路径字段名不同，
用 `nascli list` 找到方法后检查 schema 中该请求类型的字段名并修正 `path_field` 映射。

- [ ] **Step 7: 提交**

```bash
git add src/commands/files.rs src/main.rs tests/files_cmd.rs
git commit -m "feat(fs): file operations with unary listing workaround"
```

---

### Task 13: Claude Code 技能

**Files:**
- Create: `D:\Code\.claude\skills\nascli\SKILL.md`
- Create: `D:\Code\.claude\skills\nascli\DECISIONS.md`

**Interfaces:**
- Consumes: 已安装的 `nascli` 二进制
- Produces: 技能文档

- [ ] **Step 1: 写 `SKILL.md`**

```markdown
---
name: nascli
description: 通过 nascli 查询绿联 NAS 的存储、系统指标、Docker、文件与应用状态。Use when 用户问 NAS 磁盘/容量/温度/CPU/内存、要看容器或 Compose 项目、要查 NAS 上的文件、要确认某个服务是否在跑，或提到「NAS 状态」「NAS 上有什么」时。
---

# NAS CLI 使用指南

用 `nascli` 查询绿联 NAS。它是只读优先的：读命令直接跑，写命令必须加 `--yes`。

## 权限边界（硬约束）

- ✅ **默认只用只读命令**（`list --read-only` 里列出的）
- ❌ **写 / 删 / 执行类命令**（停容器、删文件、删 Compose 项目）**必须先取得用户明确同意**，才可加 `--yes`
- ❌ 不把 `UGPOA_TOKEN` 写进任何文件、脚本或命令历史
- ❌ 不修改 NAS 配置

## 前置条件

```powershell
$env:UGNAS_HOST = "<NAS 地址>"
$env:UGPOA_TOKEN = "<访问令牌>"
```

## 常用命令

| 场景 | 命令 |
|---|---|
| 磁盘列表 | `nascli storage disks` |
| 存储卷与容量 | `nascli storage volumes` |
| 存储池 | `nascli storage pools` |
| 磁盘健康 | `nascli storage smart` |
| CPU 使用率 | `nascli system cpu` |
| 内存使用 | `nascli system mem` |
| 网络流量 | `nascli system net` |
| 系统版本与型号 | `nascli system identity` |
| 运行总览 | `nascli system overview` |
| 进程占用 | `nascli system processes` |
| 服务占用 | `nascli system services` |
| UPS 状态 | `nascli system ups` |
| 风扇 / 电源 / LED | `nascli system fan` / `power` / `led` |
| 网络配置 | `nascli network general` |
| 容器列表 | `nascli docker containers` |
| 本地镜像 | `nascli docker images` |
| Compose 项目 | `nascli docker compose` |
| 容器日志 | `nascli docker logs` |
| 应用列表 | `nascli apps list` |
| 用户与用户组 | `nascli users list` / `nascli users groups` |
| 下载任务 | `nascli download tasks` |
| 文件属性 | `nascli fs stat --path <p>` |
| 列目录 | `nascli fs ls --path <p> --limit 50` |

## 发现更多能力

```bash
nascli list --read-only      # 所有可安全使用的方法
nascli list                  # 全部方法（含写类）
nascli <子命令> --help       # 某个子命令的参数
```

## 不支持的能力

以下方法在服务端是**流式 RPC**，OpenCLI 的 HTTP 接口不支持，会返回退出码 5：

- `fs du`（目录容量统计）
- `fs find`（文件搜索）
- 底层 `Readdir`（已用 `CategoryOrFileDirList` 替代）

遇到退出码 5 时不要重试，直接告知用户该能力不可用。

## 输出与退出码

- stdout 永远是 JSON：成功 `{"ok":true,...}`，失败 `{"ok":false,"error":{...}}`
- 退出码：`0` 成功 / `2` 参数错误 / `3` 令牌无效 / `4` 无权限 / `5` 不支持 / `6` 网络错误 / `7` 服务端错误

## 典型用法

用户问「NAS 空间还够吗」：

```bash
nascli storage volumes
```

用户问「哪些容器在跑」：

```bash
nascli docker containers
```

用户问「NAS 卡不卡」：

```bash
nascli system cpu
nascli system mem
nascli system overview
```
```

- [ ] **Step 2: 写 `DECISIONS.md`**

```markdown
# nascli 技能决策记录

## 为什么技能只列高频命令

CLI 覆盖 101 个方法，但技能文档只写 20 条左右。
冷门方法交给 `nascli list` 与 `--help` 发现，避免技能文档膨胀挤占智能体上下文。

## 为什么不支持 du / find

这两个方法在服务端是流式 RPC，OpenCLI 的 HTTP 接口只支持一元调用，
调用会返回 `cardinality violation`。硬啃需要实现流式分帧，收益低，故标记不支持。

## 为什么写操作要 --yes

令牌是管理员级、永不过期的全能凭证。默认只读 + 显式确认，
把「智能体误操作」的风险挡在 CLI 层，而不是依赖提示词约束。

## 为什么令牌只走环境变量

避免明文落盘。技能里也不写令牌值，只写变量名。
```

- [ ] **Step 3: 验证技能可被加载**

```bash
ls "D:/Code/.claude/skills/nascli/"
```
Expected: `SKILL.md` 与 `DECISIONS.md`

- [ ] **Step 4: 提交（技能在 D:\Code，非 nascli 仓库）**

```bash
cd D:/Code && git status --short .claude/skills/nascli
```
Expected: 若 `D:\Code` 非 git 仓库则跳过提交，仅保留文件

---

### Task 14: README 与 fixture 脱敏

**Files:**
- Create: `README.md`
- Create: `tests/fixtures/README.md`
- Modify: `tests/` 中引用 fixture 的测试

**Interfaces:**
- Consumes: 全部已完成模块
- Produces: 可公开发布的仓库

- [ ] **Step 1: 写 `README.md`**

````markdown
# nascli

绿联 UGOS Pro「访问令牌」（OpenCLI）的命令行客户端。
默认只读，适合交给 AI 智能体安全使用。

## 它解决什么问题

UGOS Pro 的访问令牌背后是一套 gRPC-over-HTTP 接口，请求体是 protobuf 二进制。
裸用对人和智能体都不友好。nascli 把它封装成语义化命令，并强制「读直接跑、写要确认」。

## 安装

```bash
# 1. 生成 schema（需要到 NAS 的 SSH 访问）
python tools/extract_schema.py --host nas.example.com

# 2. 构建
cargo build --release
cargo install --path .
```

## 配置

只认环境变量，不写配置文件：

```powershell
$env:UGNAS_HOST = "nas.example.com"   # 必填，支持 IP / DDNS / UGREENlink ID
$env:UGPOA_TOKEN = "ugpoa_v1...."     # 必填
$env:UGNAS_PORT = "9443"              # 可选，默认 9443
$env:UGNAS_INSECURE = "1"             # 可选，默认接受自签名证书
```

## 用法

```bash
nascli storage volumes          # 存储卷与容量
nascli system cpu               # CPU 使用率
nascli docker containers        # 容器列表
nascli fs ls --path /volume1/demo --limit 20
nascli list --read-only         # 列出所有只读方法
nascli call <socket> <method> --field 3:string=/volume1/demo
```

## 安全模型

- 读方法直接执行；写 / 删 / 执行类必须加 `--yes`
- 令牌只从环境变量读取，不落盘、不写日志
- stdout 只输出 JSON，便于程序化处理

## 已知限制

服务端流式方法（`du`、`find`、底层 `Readdir`）不被 HTTP 接口支持，调用返回退出码 5。

## 许可

MIT
````

- [ ] **Step 2: 写 fixture 脱敏说明 `tests/fixtures/README.md`**

```markdown
# 测试样本

这些 `.bin` 是 OpenCLI 响应的 protobuf 样本，用于离线快照测试。

**所有样本均已脱敏**：字段号、类型、嵌套层级保持原样，字符串内容替换为占位符。
不得提交任何含真实主机名、IP、序列号、用户名、邮箱或真实路径的样本。

脱敏方法：保留结构，把字符串字段替换为等长的占位内容
（例如用户家目录路径 → `/volume1/demo`）。
```

- [ ] **Step 3: 扫描仓库确认无真实环境信息**

```bash
grep -rniE "192[.]168[.]|WWD[0-9A-Z]{5}|@qq[.]com|/volume1/@hom[e]/" \
  --include="*.rs" --include="*.py" --include="*.md" --include="*.toml" .
```
Expected: 无输出

- [ ] **Step 4: 跑全量测试**

```bash
cargo test
```
Expected: 全部通过

- [ ] **Step 5: 提交**

```bash
git add README.md tests/fixtures/README.md
git commit -m "docs: add README and fixture sanitization policy"
```

---

## 完成标准

- [ ] `cargo test` 全绿
- [ ] `cargo build --release` 成功
- [ ] `nascli list` 输出 101 个方法
- [ ] `nascli list --read-only` 只输出 read 类方法
- [ ] 写类命令无 `--yes` 时退出码为 2
- [ ] 流式方法返回退出码 5
- [ ] 仓库内无真实环境信息
- [ ] `D:\Code\.claude\skills\nascli\SKILL.md` 存在且可被 Claude Code 加载
