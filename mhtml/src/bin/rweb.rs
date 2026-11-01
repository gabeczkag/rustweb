use std::fs;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use mhtml::{BuildOptions, LinkerConfig};

#[derive(Debug, Parser)]
#[command(
    name = "rweb",
    version,
    about = "Compile MHTML projects into one self-contained .rweb HTML file"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compile a linker.json project into a .rweb file.
    Build {
        /// linker.json or a directory containing it.
        linker: PathBuf,

        /// Output .rweb path (defaults to linker.output or site.rweb).
        #[arg(short, long)]
        out: Option<PathBuf>,

        /// Disable HTML minification.
        #[arg(long)]
        no_minify: bool,
    },

    /// Inspect a compiled .rweb file.
    Inspect {
        /// .rweb file to inspect.
        file: PathBuf,
    },
}

fn main() -> Result<(), mhtml::Error> {
    match Cli::parse().command {
        Command::Build {
            linker,
            out,
            no_minify,
        } => {
            let linker_path = if linker.is_dir() {
                linker.join("linker.json")
            } else {
                linker
            };
            let config = LinkerConfig::load(&linker_path)?;
            let root = linker_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
            let output = out
                .or_else(|| config.output.clone().map(|path| root.join(path)))
                .unwrap_or_else(|| root.join("site.rweb"));

            let bundle = mhtml::build(&linker_path, BuildOptions { minify: !no_minify })?;
            fs::write(&output, &bundle.html)?;

            println!("built {}", output.display());
            println!(
                "pages: {}  styles: {}  scripts: {}  assets: {}",
                bundle.stats.pages, bundle.stats.styles, bundle.stats.scripts, bundle.stats.assets
            );
            println!(
                "source: {} -> rweb: {} ({:.1}%)",
                human(bundle.stats.source_bytes),
                human(bundle.stats.bytes),
                ratio(bundle.stats.bytes, bundle.stats.source_bytes)
            );
        }
        Command::Inspect { file } => {
            let html = fs::read_to_string(&file)?;
            let info = mhtml::inspect(&html);

            println!(
                "{} — {}",
                info.title.as_deref().unwrap_or("untitled"),
                info.version.as_deref().unwrap_or("unknown")
            );
            if let Some(protocol) = &info.protocol {
                println!(
                    "protocol: {} v{} (https={})",
                    protocol.name, protocol.version, protocol.https
                );
            }
            println!("{} bytes, {} routes", info.bytes, info.routes.len());
            for (route, title) in info.routes {
                println!(
                    "  {:<20} {}",
                    route,
                    title.unwrap_or_else(|| "(untitled)".to_owned())
                );
            }
        }
    }

    Ok(())
}

fn human(bytes: usize) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn ratio(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64 * 100.0
    }
}
