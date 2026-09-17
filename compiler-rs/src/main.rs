use clap::{Parser, Subcommand};
use compiler_rs::server::DevServer;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "com-compiler")]
#[command(about = "High-performance App Router & Server Actions compiler & dev server in Rust")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the development & preview server
    Dev {
        /// Project root directory
        #[arg(short, long, default_value = ".")]
        dir: PathBuf,

        /// Port to listen on
        #[arg(short, long, default_value_t = 3000)]
        port: u16,

        /// Target runtime mode (web | zalo)
        #[arg(short, long, default_value = "web")]
        target: String,
    },
    /// Scan App Router structure
    Routes {
        /// Project root directory
        #[arg(short, long, default_value = ".")]
        dir: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Dev { dir, port, target } => {
            println!("\x1b[1;36m◆ COM.AI.VN RUST COMPILER & DEV SERVER\x1b[0m");
            println!("  Root:   {:?}", dir.canonicalize().unwrap_or(dir.clone()));
            println!("  Target: \x1b[1;32m{}\x1b[0m", target);
            println!();

            DevServer::run(dir, port, target).await?;
        }
        Commands::Routes { dir } => {
            let router = compiler_rs::router::AppRouter::scan(&dir);
            println!("\x1b[1;36m◆ DISCOVERED APP ROUTER ROUTES:\x1b[0m");
            for r in &router.routes {
                let kind = if r.is_api { "API " } else { "PAGE" };
                println!("  [{}] {} -> {:?}", kind, r.pattern, r.page_file);
            }
        }
    }

    Ok(())
}
