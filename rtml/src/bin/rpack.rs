use std::path::PathBuf;

use clap::{Parser, Subcommand};
use rtml::{build_dir, SitePack};

#[derive(Debug, Parser)]
#[command(
    name = "rpack",
    version,
    about = "Build and inspect .rpack site packages for RustWeb"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Build a site directory into a single .rpack package.
    Build {
        /// Directory containing site.json, RTML pages, and assets/.
        site: PathBuf,

        /// Output package path.
        #[arg(short, long, default_value = "site.rpack")]
        out: PathBuf,
    },

    /// Inspect the contents of a .rpack package.
    Inspect {
        /// Package to inspect.
        pack: PathBuf,
    },
}

fn main() -> Result<(), rtml::Error> {
    match Cli::parse().command {
        Command::Build { site, out } => {
            let report = build_dir(&site, &out)?;
            let stats = &report.stats;

            println!("built {}", report.output.display());
            println!("pages: {}  assets: {}", stats.pages, stats.assets);
            println!(
                "source: {} -> html: {} + assets: {} (unpacked: {})",
                human(stats.source_bytes),
                human(stats.html_bytes),
                human(stats.asset_bytes),
                human(stats.unpacked_bytes)
            );
            println!(
                "packed: {} ({:.1}% of unpacked)",
                human(stats.packed_bytes),
                ratio(stats.packed_bytes, stats.unpacked_bytes)
            );
        }
        Command::Inspect { pack } => {
            let pack = SitePack::load(&pack)?;
            let stats = pack.stats();

            println!("{} — {}", pack.site().title, pack.site().version);
            if !pack.site().description.is_empty() {
                println!("{}", pack.site().description);
            }
            println!(
                "pages: {}  assets: {}  packed: {} / unpacked: {}",
                stats.pages,
                stats.assets,
                human(stats.packed_bytes),
                human(stats.unpacked_bytes)
            );

            for page in pack.pages() {
                println!(
                    "  {:<24} {:>8}{}",
                    page.route,
                    human(page.html.len()),
                    page.title
                        .as_ref()
                        .map(|title| format!("  {title}"))
                        .unwrap_or_default()
                );
            }

            for asset in pack.assets() {
                println!(
                    "  {:<24} {:>8}  {}",
                    asset.path,
                    human(asset.bytes.len()),
                    asset.content_type
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
