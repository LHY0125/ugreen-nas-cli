use std::path::Path;

fn main() {
    let schema = Path::new("spec/schema.json");
    println!("cargo:rerun-if-changed=spec/schema.json");
    if !schema.exists() {
        panic!(
            "\n\n缺少 spec/schema.json。\n\
             该文件应随仓库提供（已入库），请检查是否被误删：\n\
               git checkout -- spec/schema.json\n\
             仅在固件升级、需要重新提取时才运行：\n\
               python tools/extract_schema.py --host <NAS 地址>\n"
        );
    }
}
