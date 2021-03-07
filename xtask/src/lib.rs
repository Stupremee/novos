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
    env!("CARGO_MANIFEST_DIR").into()
}

/// Build the NovOS kernel using the given configuration.
pub fn build(cfg: Config) -> Result<()> {
    let release = if cfg.release { &["--release"] } else { &[][..] };

    cmd!("cargo build -p kernel {release...}").run()?;
    Ok(())
}

/// Run the build kernel using QEMU.
pub fn run(cfg: Config) -> Result<()> {
    let cpus = cfg.cpus.to_string();
    let ram = cfg.ram;
    let debug = if cfg.release {
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
            -bios opensbi-riscv64-generic-fw_jump.bin 
            -kernel {path}
            -monitor stdio
            -gdb tcp::1234
            -S 
            {debug...}
    "
    )
    .run()?;

    Ok(())
}
