use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=DOURI_PAYLOAD_ZIP");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest_payload = out_dir.join("payload.zip");

    if let Ok(src) = env::var("DOURI_PAYLOAD_ZIP") {
        let src_path = PathBuf::from(src);
        if src_path.is_file() {
            println!("cargo:rerun-if-changed={}", src_path.display());
            fs::copy(&src_path, &dest_payload).expect("failed to copy payload.zip");
            return;
        }
    }

    // Default or fallback: write a valid empty zip archive (22 bytes) so compilation never fails
    if !dest_payload.exists() {
        const EMPTY_ZIP: [u8; 22] = [
            0x50, 0x4B, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        fs::write(&dest_payload, EMPTY_ZIP).expect("failed to write empty fallback payload.zip");
    }
}
