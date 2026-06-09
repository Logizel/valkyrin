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
    Sync,
}

#[tokio::main]
async fn main() {
    // 1. The Panic Boundary
    // Intercepts fatal thread crashes and replaces the ugly Rust stack trace
    // with a highly readable, color-coded terminal message.
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
    // Catches standard Results (e.g., File Not Found, Database Offline)
    if let Err(e) = execute_command(cli.command).await {
        eprintln!("\n{} {}", "Error:".red().bold(), e);

        // Print the cascade of underlying errors if they exist
        let mut cause = e.source();
        while let Some(err) = cause {
            eprintln!("  {} {}", "↳".dimmed(), err);
            cause = err.source();
        }
        std::process::exit(1);
    }
}

async fn execute_command(command: Commands) -> Result<()> {
    match command {
        Commands::Init => {
            println!("{} Initializing Valkyrin workspace...", "=>".green().bold());
            // Initialization logic
        }
        Commands::Canvas => {
            println!(
                "{} Booting Canvas on http://localhost:3000...",
                "=>".blue().bold()
            );
            // Server boot logic
        }
        Commands::Generate => {
            println!(
                "{} Compiling blueprint to structural code...",
                "=>".magenta().bold()
            );
            // Code generation logic
        }
        Commands::Sync => {
            println!(
                "{} Synchronizing with live database catalog...",
                "=>".cyan().bold()
            );
            // Database polling logic
        }
    }
    Ok(())
}
