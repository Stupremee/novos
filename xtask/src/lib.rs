use clap::Clap;
use color_eyre::Result;
use std::path::PathBuf;
use xshell::cmd;

/// Build system and shortcut commands for NovOS.
#[derive(Clap)]
pub enum Arguments {
    /// Build the OpenSBI firmware using Nix.
    OpenSBI,
    /// Build NovOS and run it via QEMU.
    Run(Config),
    /// Build NovOS without running it.
    Build(Config),
    /// Run cargo-watch and check every project
    Watch,
}

/// Configure QEMU and build of NovOS.
#[derive(Clap, Clone)]
pub struct Config {
    /// The amount of RAM to give to QEMU.
    #[clap(long, default_value = "512M")]
    pub ram: String,
    /// The number of harts to boot.
    #[clap(long, default_value = "4")]
    pub cpus: u32,
    /// Enable QEMU debug messages.
    #[clap(short, long)]
    pub debug: bool,
    /// Build kernel in release mode.
    #[clap(short, long)]
    pub release: bool,
}

/// Returns the path to the root of the project.
pub fn root() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path
}

/// Build OpenSBI firmware
pub fn opensbi() -> Result<()> {
    cmd!("nix-build nix/opensbi.nix").run()?;
    Ok(())
}

/// Build the NovOS kernel using the given configuration.
pub fn build(cfg: Config) -> Result<()> {
    let _flags = xshell::pushenv("RUSTFLAGS", "-Clink-arg=-Tcrates/kernel/lds/qemu.lds");
    let release = if cfg.release { &["--release"] } else { &[][..] };

    cmd!(
        "cargo build --target riscv64gc-unknown-none-elf -Zbuild-std=core -p kernel {release...}"
    )
    .run()?;
    Ok(())
}

/// Run the build kernel using QEMU.
pub fn run(cfg: Config) -> Result<()> {
    let cpus = cfg.cpus.to_string();
    let ram = cfg.ram;
    let debug = if cfg.debug {
        &["-d", "guest_errors,trace:riscv_trap,int"]
    } else {
        &[][..]
    };
    let path = if cfg.release {
        "target/riscv64gc-unknown-none-elf/release/kernel"
    } else {
        "target/riscv64gc-unknown-none-elf/debug/kernel"
    };

    cmd!(
        "
        qemu-system-riscv64 
            -machine virt
            -cpu rv64
            -smp {cpus}
            -m {ram}
            -nographic
            -bios result/platform/fw_jump.bin
            -kernel {path}
            -gdb tcp::1234
            {debug...}
    "
    )
    .run()?;

    Ok(())
}

pub fn watch() -> Result<()> {
    let metadata = cmd!("cargo metadata").read()?;
    let crates = cmd!("jq -r '.packages | .[].manifest_path'")
        .stdin(metadata)
        .read()?;

    let mut args = vec![];

    for c in crates.lines() {
        if c.starts_with(xshell::cwd()?.to_str().unwrap()) && !c.contains("xtask") {
            let mut pkg = PathBuf::from(c);
            pkg.pop();
            let pkg = pkg.file_name().unwrap();

            let arg = format!("-x clippy -p {}", pkg.to_str().unwrap());
            args.push(arg);
            let arg = format!("-x doc -p {}", pkg.to_str().unwrap());
            args.push(arg);
        }
    }

    cmd!("cargo watch -c {args...}").run()?;

    Ok(())
}
