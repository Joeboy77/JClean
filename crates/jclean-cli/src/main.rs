//! Dev/test harness for `jclean-core`. Scan and clean commands arrive in phase 1.

fn main() -> anyhow::Result<()> {
    let arg = std::env::args().nth(1);
    match arg.as_deref() {
        None | Some("--version" | "-V") => {
            println!("jclean-cli {}", jclean_core::VERSION);
            Ok(())
        }
        Some(other) => anyhow::bail!("unknown command: {other}"),
    }
}
