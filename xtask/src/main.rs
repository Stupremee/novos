use clap::Clap;
use xtask::Arguments;

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let _guard = xshell::pushd(xtask::root());
    let args = Arguments::parse();

    match args {
        Arguments::OpenSBI => xtask::opensbi()?,
        Arguments::Run(cfg) => {
            xtask::build(cfg.clone())?;
            xtask::run(cfg)?;
        }
        Arguments::Build(cfg) => xtask::build(cfg)?,
        Arguments::Watch => xtask::watch()?,
    }

    Ok(())
}
