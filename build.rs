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
