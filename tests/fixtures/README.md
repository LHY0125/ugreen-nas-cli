# 测试样本

当前仓库**没有二进制 fixture**。所有测试使用内联合成的 protobuf 数据
（在 `tests/*.rs` 与 `tools/tests/test_extract.py` 里直接构造字节），
不依赖任何从真实 NAS 抓取的响应。

## 脱敏政策

如果将来加入 `.bin` 样本（例如 OpenCLI 响应的 protobuf 快照），必须遵守：

- **结构保留**：字段号、wire type、嵌套层级、repeated 标记保持原样。
- **内容替换**：所有字符串字段替换为占位内容，长度可以不同。
- **禁止提交**：真实主机名、IP、磁盘序列号、用户名、邮箱、用户家目录路径、令牌值。

替换示例：

| 原始内容类型 | 替换为 |
| --- | --- |
| 主机名 / IP | `nas.example.com` |
| 共享目录或用户目录 | `/volume1/demo` |
| 磁盘序列号 | `<serial>` |
| 用户名 | `<user>` |
| 令牌 | `ugpoa_v1.<redacted>` |

提交前请用仓库根目录的脱敏扫描命令自检（见 `README.md` 与实现计划中的扫描步骤）。
