use clap::{Parser, Subcommand};
use compiler_rs::server::DevServer;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// mimalloc: lower fragmentation and RSS for a long-running dev/prod server than the system allocator
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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

/// `com dev` serves one developer: two runtime threads are plenty and every extra thread costs stack and
/// buffers. Production serving (`com start`) will size the runtime from the CPU count.
fn runtime_threads() -> usize {
    std::env::var("COM_THREADS").ok().and_then(|v| v.parse().ok()).filter(|n: &usize| *n > 0).unwrap_or(2)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(runtime_threads())
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
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
            compiler_rs::term::mark_start();
            if std::env::var("COM_MANAGED").is_err() {
                println!("\n   \x1b[1m▲ com.ai.vn\x1b[0m \x1b[90m(Rust engine)\x1b[0m");
                println!("   - Target:       {}", target);
            }

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
