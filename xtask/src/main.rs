use anyhow::{bail, Result};
use pico_args::Arguments;
use std::path::PathBuf;
use xshell::cmd;

const HELP: &str = "\
xtask
    The build system and 'Makefile' for NovOS

FLAGS:
    -h, --help      Print this message.
    -d, --debug     Enable QEMU debug messaging.
    --no-release    Build the kernel without optimizations.
    --cpus          Set the number CPU cores (default: 4).
    --ram           Set the amount of RAM (default: 512M).
    --gdb           Start QEMU with GDB server enabled and waiting for a connection.

SUBCOMMANDS:
    opensbi         Build the OpenSBI firmware using Nix.
    build           Build the NovOS kernel without running it.
    run             Build and run the NovOS kernel using QEMU.
";

fn main() -> Result<()> {
    let mut args = Arguments::from_env();

    // print help message if requested
    if args.contains(["-h", "--help"]) {
        print!("{}", HELP);
        return Ok(());
    }

    // cd into the root folder of this workspace
    let _cwd = xshell::pushd(root());

    match args.subcommand()?.as_deref() {
        Some("opensbi") => cmd!("nix-build nix/opensbi.nix").run()?,
        Some("build") => {
            // build the kernel
            let no_release = args.contains("--no-release");
            build(no_release)?;
        }
        Some("run") => {
            // first build the kernel
            let no_release = args.contains("--no-release");
            build(no_release)?;

            // then run the produced binray in QEMU
            run(no_release, args)?;
        }

        Some(cmd) => bail!("Unknown subcommand: '{}'", cmd),
        None => bail!("You must supply a subcommand."),
    }

    Ok(())
}

/// Run the kernel using QEMU.
fn run(no_release: bool, mut args: Arguments) -> Result<()> {
    let ram = args
        .opt_value_from_str::<_, String>("--ram")?
        .unwrap_or_else(|| "512M".to_string());
    let cpus = args
        .opt_value_from_str::<_, u32>("--cpus")?
        .unwrap_or(4)
        .to_string();
    let machine = args
        .opt_value_from_str::<_, String>("--machine")?
        .unwrap_or_else(|| "virt".to_string());

    let path = if no_release {
        "target/riscv64gc-unknown-none-elf/debug/kernel"
    } else {
        "target/riscv64gc-unknown-none-elf/release/kernel"
    };

    let debug = if args.contains(["-d", "--debug"]) {
        &["-d", "guest_errors,trace:riscv_trap"]
    } else {
        &[][..]
    };

    let gdb = if args.contains("--gdb") {
        &["-gdb", "tcp::1234", "-S"]
    } else {
        &[][..]
    };

    cmd!(
        "
        qemu-system-riscv64 
            -machine {machine}
            -cpu rv64
            -smp {cpus}
            -m {ram}
            -nographic
            -bios result/platform/fw_jump.bin
            -kernel {path}
            {gdb...}
            {debug...}
    "
    )
    .run()?;

    Ok(())
}

/// Build the kernel
fn build(no_release: bool) -> Result<()> {
    let _flags = xshell::pushenv("RUSTFLAGS", "-Clink-arg=-Tcrates/kernel/lds/qemu.lds");
    let release = if no_release { &[][..] } else { &["--release"] };

    cmd!(
        "cargo build --target riscv64gc-unknown-none-elf -Zbuild-std=core,alloc -p kernel {release...}"
    )
    .run()?;
    Ok(())
}

/// Returns the path to the root of the project.
fn root() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path
}
