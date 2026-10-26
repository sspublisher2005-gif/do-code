use std::{env, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=RUSTC");
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    if let Ok(output) = Command::new(rustc).arg("--version").output()
        && output.status.success()
    {
        let version = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or("unknown rustc version")
            .trim()
            .to_owned();
        println!("cargo:rustc-env=RUSTC_VERSION={version}");
    }
}
