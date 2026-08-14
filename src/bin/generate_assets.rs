use std::fs;
use std::io::Error;
use std::path::Path;

use clap::CommandFactory;
use clap_complete::{Shell, generate_to};
use clap_mangen::Man;

use kat::cli::Cli;

fn main() -> Result<(), Error> {
    let man_dir = Path::new("generated/man");
    let comp_dir = Path::new("generated/completions");

    fs::create_dir_all(man_dir)?;
    fs::create_dir_all(comp_dir)?;

    let mut cmd = Cli::command();
    cmd.set_bin_name("kat");

    // 1. Generate man pages
    println!("Generating UNIX man pages in generated/man/...");
    render_man_pages(&cmd, man_dir)?;

    // 2. Generate shell completions
    println!("Generating shell completions in generated/completions/...");
    let mut cmd_for_comp = cmd.clone();
    generate_to(Shell::Bash, &mut cmd_for_comp, "kat", comp_dir)?;
    generate_to(Shell::Zsh, &mut cmd_for_comp, "kat", comp_dir)?;
    generate_to(Shell::Fish, &mut cmd_for_comp, "kat", comp_dir)?;

    println!("Asset generation completed successfully!");
    Ok(())
}

fn render_man_pages(cmd: &clap::Command, out_dir: &Path) -> Result<(), Error> {
    let man = Man::new(cmd.clone());
    let mut buffer = Vec::new();
    man.render(&mut buffer)?;
    let root_path = out_dir.join("kat.1");
    fs::write(&root_path, buffer)?;

    for sub in cmd.get_subcommands() {
        let sub_name = sub.get_name();
        if sub_name == "help" {
            continue;
        }
        let sub_man = Man::new(sub.clone());
        let mut sub_buf = Vec::new();
        sub_man.render(&mut sub_buf)?;
        let sub_path = out_dir.join(format!("kat-{sub_name}.1"));
        fs::write(&sub_path, sub_buf)?;
    }

    Ok(())
}
