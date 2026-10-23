use clap::{Parser, Subcommand};
use compiler_rs::server::DevServer;
use std::path::PathBuf;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// mimalloc: lower fragmentation and RSS for a long-running dev/prod server than the system allocator
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser, Debug)]
#[command(name = "com-compiler", version)]
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
    /// Production build into .nata/build
    Build {
        #[arg(short, long, default_value = ".")]
        dir: PathBuf,
        #[arg(short, long, default_value = "web")]
        target: String,
    },
    /// Serve the production build
    Start {
        #[arg(short, long, default_value = ".")]
        dir: PathBuf,
        #[arg(short, long, default_value_t = 3000)]
        port: u16,
        /// Interface to listen on
        #[arg(short = 'H', long, default_value = "0.0.0.0")]
        hostname: String,
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
/// buffers. Production (`com build`, `com start`) uses every core.
fn runtime_threads(production: bool) -> usize {
    let default = if production { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) } else { 2 };
    std::env::var("COM_THREADS").ok().and_then(|v| v.parse().ok()).filter(|n: &usize| *n > 0).unwrap_or(default)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let production = matches!(cli.command, Commands::Build { .. } | Commands::Start { .. });
    if production {
        // Before any thread starts: the worker and every module compiled inherit it
        compiler_rs::mode::set_production();
    }
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    // Rayon (Tailwind class scanning) starts one thread per core by default and keeps them for good: a
    // scan in `com dev` touches a few files at a time, so four are enough there
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(if production { cores } else { cores.min(4) })
        .thread_name(|i| format!("rayon-{i}"))
        .build_global();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(runtime_threads(production))
        // Blocking work (file reads, SWC, compression) is bounded by the cores anyway; tokio's default
        // of 512 threads lets a cold page burst spawn dozens, each with its own stack
        .max_blocking_threads(if production { (cores * 4).max(16) } else { 16 })
        .enable_all()
        .build()?
        .block_on(run(cli))
}

async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    match cli.command {
        Commands::Dev { dir, port, target } => {
            compiler_rs::term::mark_start();
            if std::env::var("COM_MANAGED").is_err() {
                println!("\n   \x1b[1m▲ com.ai.vn\x1b[0m \x1b[90m(Rust engine)\x1b[0m");
                println!("   - Target:       {}", target);
            }

            DevServer::run(dir, port, target).await?;
        }
        Commands::Build { dir, target } => {
            if std::env::var("COM_MANAGED").is_err() {
                println!("\n   \x1b[1m▲ com.ai.vn\x1b[0m \x1b[90m(production build)\x1b[0m\n");
            }
            if let Err(err) = compiler_rs::build::run(dir, target).await {
                compiler_rs::term::error("Build failed", Some(&err), None);
                compiler_rs::worker_pool::shutdown_all().await;
                std::process::exit(1);
            }
            compiler_rs::worker_pool::shutdown_all().await;
        }
        Commands::Start { dir, port, hostname, target } => {
            compiler_rs::term::mark_start();
            if std::env::var("COM_MANAGED").is_err() {
                println!("\n   \x1b[1m▲ com.ai.vn\x1b[0m \x1b[90m(production)\x1b[0m");
            }
            if let Err(err) = compiler_rs::prod::run(dir, port, hostname, target).await {
                compiler_rs::term::error("Could not start", Some(&err), None);
                std::process::exit(1);
            }
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
