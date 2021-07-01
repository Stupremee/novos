use anyhow::{bail, Result};
use pico_args::Arguments;
use std::os::unix::process::CommandExt;
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
    --ram           Set the amount of RAM in megabytes (default: 512).
    --gdb           Start QEMU with GDB server enabled and waiting for a connection.
    --spike         Run the kernel using spike instead of QEMU.

SUBCOMMANDS:
    opensbi         Build the OpenSBI firmware using Nix.
    build           Build the NovOS kernel without running it.
    run             Build and run the NovOS kernel using QEMU.
    watch           Use cargo-watch to check the kernel.
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
        Some("watch") => watch()?,
        Some("build") => {
            // build the kernel
            let no_release = args.contains("--no-release");
            let spike = args.contains("--spike");
            build(no_release, spike)?;
        }
        Some("run") => {
            // first build the kernel
            let spike = args.contains("--spike");
            let no_release = args.contains("--no-release");
            build(no_release, spike)?;

            // then run the produced binray in QEMU
            run(no_release, spike, args)?;
        }

        Some(cmd) => bail!("Unknown subcommand: '{}'", cmd),
        None => bail!("You must supply a subcommand."),
    }

    Ok(())
}

/// Run the kernel using QEMU.
pub fn run(no_release: bool, spike: bool, mut args: Arguments) -> Result<()> {
    let ram = args
        .opt_value_from_str::<_, String>("--ram")?
        .unwrap_or_else(|| "512".to_string());
    let cpus = args
        .opt_value_from_str::<_, u32>("--cpus")?
        .unwrap_or(4)
        .to_string();
    let machine = args
        .opt_value_from_str::<_, String>("--machine")?
        .unwrap_or_else(|| "virt".to_string());

    let mut path = if no_release {
        "target/riscv64gc-unknown-none-elf/debug/kernel".to_string()
    } else {
        "target/riscv64gc-unknown-none-elf/release/kernel".to_string()
    };

    let mut opensbi = PathBuf::from(std::env::var("OPENSBI")?);
    opensbi.push("platform");
    opensbi.push("fw_jump.elf");

    if spike {
        path += ".bin";
    }

    let debug = if args.contains(["-d", "--debug"]) {
        &["-d", "guest_errors,trace:riscv_trap"]
    } else {
        &[][..]
    };

    let gdb = if args.contains("--gdb") {
        if spike {
            &["-d"][..]
        } else {
            &["-gdb", "tcp::1234", "-S"][..]
        }
    } else {
        &[][..]
    };

    if spike {
        let mut cmd: std::process::Command = cmd!(
            "
            spike
                -p{cpus}
                -m{ram}
                --kernel={path}
                {gdb...}
                {opensbi}
        "
        )
        .into();
        return Err(cmd.exec().into());
    } else {
        cmd!(
            "
            qemu-system-riscv64 
                -machine {machine}
                -cpu rv64
                -smp {cpus}
                -m {ram}M
                -nographic
                -bios {opensbi}
                -kernel {path}
                {gdb...}
                {debug...}
        "
        )
        .run()?;
    }

    Ok(())
}

/// Build the kernel
fn build(no_release: bool, spike: bool) -> Result<()> {
    let release = if no_release { &[][..] } else { &["--release"] };

    cmd!("cargo build -p kernel {release...}").run()?;

    let path = if no_release {
        "target/riscv64gc-unknown-none-elf/debug/kernel"
    } else {
        "target/riscv64gc-unknown-none-elf/release/kernel"
    };

    if spike {
        cmd!("llvm-objcopy --set-start=0x80200000 -O binary {path} {path}.bin").run()?;
    }
    Ok(())
}

fn watch() -> Result<()> {
    cmd!("cargo watch -c -x 'clippy -p kernel' -x 'doc -p kernel'").run()?;
    Ok(())
}

/// Returns the path to the root of the project.
fn root() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path
}
