// valkyrin-cli/src/main.rs
use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;
use std::panic;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init,
    Canvas,
    Generate,
    Sync {
        /// Database connection string (auto-detects type from URL prefix)
        #[arg(short, long)]
        url: String,
        /// Override auto-detection: 'postgres', 'mysql', or 'sqlite'
        #[arg(short, long)]
        db_type: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    // 1. The Panic Boundary
    panic::set_hook(Box::new(|info| {
        eprintln!("\n{}", " FATAL SYSTEM ERROR ".on_red().white().bold());
        if let Some(s) = info.payload().downcast_ref::<&str>() {
            eprintln!("\n{} {}", "Reason:".red().bold(), s);
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            eprintln!("\n{} {}", "Reason:".red().bold(), s);
        }
        if let Some(location) = info.location() {
            eprintln!(
                "{} {}:{}",
                "Location:".yellow(),
                location.file(),
                location.line()
            );
        }
        eprintln!(
            "\n{}",
            "Please report this bug on the Valkyrin GitHub repository.".dimmed()
        );
        std::process::exit(1);
    }));

    let cli = Cli::parse();

    // 2. The Expected Error Boundary
    if let Err(e) = execute_command(cli.command).await {
        eprintln!("\n{} {}", "Error:".red().bold(), e);
        for cause in e.chain().skip(1) {
            eprintln!("  {} {}", "↳".dimmed(), cause);
        }
        std::process::exit(1);
    }
}

async fn execute_command(command: Commands) -> Result<()> {
    match command {
        Commands::Init => {
            println!("{} Initializing Valkyrin workspace...", "=>".green().bold());
            valkyrin_core::config::initialize_workspace()?;
            println!(
                "{} Workspace ready! Run `valkyrin canvas` to begin.",
                "=>".cyan().bold()
            );
        }
        Commands::Canvas => {
            println!(
                "{} Booting Canvas on http://localhost:3000...",
                "=>".blue().bold()
            );
            println!("{} Press CTRL+C to stop the server.", "=>".dimmed());
            valkyrin_server::start_server(3000).await?;
        }
        Commands::Generate => {
            println!(
                "{} Compiling blueprint to structural code...",
                "=>".magenta().bold()
            );
            valkyrin_core::compiler::compile_blueprint()?;
            println!(
                "{} Generation complete. Check the 'models' folder.",
                "=>".green().bold()
            );
        }
        Commands::Sync { url, db_type } => {
            println!(
                "{} Synchronizing with live database catalog...",
                "=>".cyan().bold()
            );

            // Execute the Live DB Introspection Bridge!
            valkyrin_core::sync::SyncEngine::synchronize_database(
                &url,
                db_type.as_deref(),
            ).await?;
        }
    }
    Ok(())
}
