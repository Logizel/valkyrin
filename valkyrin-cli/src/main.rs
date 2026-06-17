// valkyrin-cli/src/main.rs
use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;
use serde_json;
use std::panic;
use valkyrin_core::error::ValkyrinError;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Output errors as JSON for CI/CD parsing
    #[arg(short, long, global = true)]
    json: bool,
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
        /// Confirm destructive changes (removing tables from canvas)
        #[arg(short, long)]
        confirm: bool,
        /// Preview changes without modifying the canvas
        #[arg(short = 'n', long)]
        dry_run: bool,
    },
    Migrate {
        /// Database connection string (auto-detects type from URL prefix)
        #[arg(short, long)]
        url: String,
        /// Override auto-detection: 'postgres', 'mysql', or 'sqlite'
        #[arg(short, long)]
        db_type: Option<String>,
        /// Path to migration file (defaults to latest in migrations/)
        #[arg(short, long)]
        file: Option<String>,
    },
    Push {
        /// Database connection string (auto-detects type from URL prefix)
        #[arg(short, long)]
        url: String,
        /// Override auto-detection: 'postgres', 'mysql', or 'sqlite'
        #[arg(short, long)]
        db_type: Option<String>,
        /// Confirm destructive changes (DROP COLUMN, etc.)
        #[arg(short, long)]
        confirm: bool,
        /// Preview changes without executing
        #[arg(short = 'n', long)]
        dry_run: bool,
    },
    Check {
        /// Database connection string (auto-detects type from URL prefix)
        #[arg(short, long)]
        url: String,
        /// Override auto-detection: 'postgres', 'mysql', or 'sqlite'
        #[arg(short, long)]
        db_type: Option<String>,
        /// Subcommand: validate
        #[command(subcommand)]
        check_command: Option<CheckCommands>,
    },
    Rollback {
        /// Database connection string (auto-detects type from URL prefix)
        #[arg(short, long)]
        url: String,
        /// Override auto-detection: 'postgres', 'mysql', or 'sqlite'
        #[arg(short, long)]
        db_type: Option<String>,
        /// Number of migrations to rollback (default: 1)
        #[arg(short, long, default_value = "1")]
        steps: usize,
        /// Preview changes without executing
        #[arg(short = 'n', long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum CheckCommands {
    Validate {
        /// Enable strict validation (exit code 2 on warnings)
        #[arg(short, long)]
        strict: bool,
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
    if let Err(e) = execute_command(cli.command, cli.json).await {
        if cli.json {
            // Output as JSON for CI/CD
            if let Some(valkyrin_err) = e.downcast_ref::<ValkyrinError>() {
                eprintln!("{}", valkyrin_err.to_json());
                std::process::exit(valkyrin_err.exit_code());
            } else {
                let json_err = serde_json::json!({
                    "code": "VAL-999",
                    "message": e.to_string(),
                    "exit_code": 2,
                });
                eprintln!("{}", json_err);
                std::process::exit(2);
            }
        } else {
            eprintln!("\n{} {}", "Error:".red().bold(), e);
            for cause in e.chain().skip(1) {
                eprintln!("  {} {}", "↳".dimmed(), cause);
            }
            std::process::exit(1);
        }
    }
}

async fn execute_command(command: Commands, _json: bool) -> Result<()> {
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
        Commands::Sync {
            url,
            db_type,
            confirm,
            dry_run,
        } => {
            let mode = if dry_run {
                valkyrin_core::sync::SyncMode::DryRun
            } else if confirm {
                valkyrin_core::sync::SyncMode::ApplyAll
            } else {
                valkyrin_core::sync::SyncMode::ApplyNew
            };

            let mode_label = match mode {
                valkyrin_core::sync::SyncMode::DryRun => "dry-run",
                valkyrin_core::sync::SyncMode::ApplyAll => "apply-all",
                valkyrin_core::sync::SyncMode::ApplyNew => "apply-new",
            };

            println!(
                "{} Synchronizing with live database catalog [{}]...",
                "=>".cyan().bold(),
                mode_label
            );

            valkyrin_core::sync::SyncEngine::synchronize_database(
                &url,
                db_type.as_deref(),
                mode,
            )
            .await?;
        }
        Commands::Migrate {
            url,
            db_type,
            file,
        } => {
            println!(
                "{} Running database migrations...",
                "=>".magenta().bold()
            );
            valkyrin_core::sync::SyncEngine::run_migrations(&url, db_type.as_deref(), file.as_deref()).await?;
        }
        Commands::Push {
            url,
            db_type,
            confirm,
            dry_run,
        } => {
            println!(
                "{} Pushing canvas changes to database...",
                "=>".green().bold()
            );
            valkyrin_core::sync::SyncEngine::push_to_database(&url, db_type.as_deref(), confirm, dry_run).await?;
        }
        Commands::Check {
            url,
            db_type,
            check_command,
        } => {
            match check_command {
                Some(CheckCommands::Validate { strict }) => {
                    println!(
                        "{} Validating schema{}...",
                        "=>".blue().bold(),
                        if strict { " (strict mode)" } else { "" }
                    );
                    valkyrin_core::validate::validate_schema(strict).await?;
                    println!("{} Schema validation passed!", "=>".green().bold());
                }
                None => {
                    println!(
                        "{} Checking database synchronization status...",
                        "=>".blue().bold()
                    );
                    valkyrin_core::sync::SyncEngine::check_sync(&url, db_type.as_deref()).await?;
                }
            }
        }
        Commands::Rollback {
            url,
            db_type,
            steps,
            dry_run,
        } => {
            println!(
                "{} Rolling back {} migration(s)...",
                "=>".yellow().bold(),
                steps
            );
            valkyrin_core::sync::SyncEngine::rollback_migrations(&url, db_type.as_deref(), steps, dry_run).await?;
        }
    }
    Ok(())
}
