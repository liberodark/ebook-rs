//! ebook-rs server entry point.

use clap::Parser;
use ebook_rs::{
    auth::AuthService,
    config::{Cli, Command, Config, LibraryCommand, UserCommand},
    db::Database,
    server,
};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Find or load config
    let config_path = cli.config.clone().or_else(Config::find_config_file);

    let config = if let Some(ref path) = config_path {
        Config::load(path)?
    } else {
        Config::default()
    };

    // Handle command
    match cli.command {
        Some(Command::Init { force }) => cmd_init(force).await,
        Some(Command::User { action }) => cmd_user(action, &config).await,
        Some(Command::Library { action }) => cmd_library(action, &config).await,
        Some(Command::Serve { bind, library }) => cmd_serve(config, bind, library).await,
        None => {
            // Default: start server
            cmd_serve(config, None, None).await
        }
    }
}

/// Initialize config and database.
async fn cmd_init(force: bool) -> anyhow::Result<()> {
    let config_path = PathBuf::from("config.toml");

    if config_path.exists() && !force {
        anyhow::bail!(
            "Config file already exists: {}. Use --force to overwrite.",
            config_path.display()
        );
    }

    // Write default config
    std::fs::write(&config_path, Config::generate_default())?;
    println!("Created config file: {}", config_path.display());

    // Initialize database
    let config = Config::default();
    if let Some(parent) = config.database.path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let _db = Database::open(&config.database.path)?;
    println!("Initialized database: {}", config.database.path.display());

    println!("\nEdit config.toml to configure your server.");
    println!("Then run: ebook-rs library add <name> --path /path/to/books");
    println!("And: ebook-rs user add <username> --password <password> --role admin");

    Ok(())
}

/// User management commands.
async fn cmd_user(action: UserCommand, config: &Config) -> anyhow::Result<()> {
    let db = Database::open(&config.database.path)?;
    let auth = AuthService::new(
        db,
        config.auth.session_days,
        config.auth.registration_enabled(),
    );

    match action {
        UserCommand::Add {
            username,
            password,
            role,
        } => {
            let password = match password {
                Some(p) => p,
                None => prompt_password("Password: ")?,
            };

            let user = auth.create_user(&username, &password, &role)?;
            println!(
                "Created user: {} (role: {}, id: {})",
                user.username, user.role, user.id
            );
        }

        UserCommand::Del { username } => {
            if auth.delete_user(&username)? {
                println!("Deleted user: {}", username);
            } else {
                println!("User not found: {}", username);
            }
        }

        UserCommand::List => {
            let users = auth.list_users()?;
            if users.is_empty() {
                println!("No users found.");
            } else {
                println!("{:<20} {:<10} {:<36} LAST LOGIN", "USERNAME", "ROLE", "ID");
                println!("{}", "-".repeat(80));
                for user in users {
                    let last_login = user
                        .last_login
                        .map(|ts| {
                            chrono::DateTime::from_timestamp(ts, 0)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                                .unwrap_or_else(|| "unknown".to_string())
                        })
                        .unwrap_or_else(|| "never".to_string());
                    println!(
                        "{:<20} {:<10} {:<36} {}",
                        user.username, user.role, user.id, last_login
                    );
                }
            }
        }

        UserCommand::Passwd { username, password } => {
            let password = match password {
                Some(p) => p,
                None => prompt_password("New password: ")?,
            };

            if auth.change_password(&username, &password)? {
                println!("Password changed for: {}", username);
            } else {
                println!("User not found: {}", username);
            }
        }
    }

    Ok(())
}

/// Library management commands.
async fn cmd_library(action: LibraryCommand, config: &Config) -> anyhow::Result<()> {
    let db = Database::open(&config.database.path)?;

    match action {
        LibraryCommand::Add { name, path, public } => {
            // Validate path
            if !path.exists() {
                anyhow::bail!("Path does not exist: {}", path.display());
            }
            if !path.is_dir() {
                anyhow::bail!("Path is not a directory: {}", path.display());
            }

            let library = ebook_rs::db::Library {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.clone(),
                path: path.to_string_lossy().to_string(),
                is_public: public,
                owner_id: None,
                created_at: ebook_rs::db::now_timestamp(),
            };

            db.create_library(&library)?;
            println!(
                "Added library: {} -> {} (public: {})",
                name,
                path.display(),
                public
            );
        }

        LibraryCommand::Del { name } => {
            if db.delete_library(&name)? {
                println!("Deleted library: {}", name);
            } else {
                println!("Library not found: {}", name);
            }
        }

        LibraryCommand::List => {
            let libraries = db.list_libraries()?;
            if libraries.is_empty() {
                println!("No libraries found.");
            } else {
                println!("{:<20} {:<50} PUBLIC", "NAME", "PATH");
                println!("{}", "-".repeat(80));
                for lib in libraries {
                    println!(
                        "{:<20} {:<50} {}",
                        lib.name,
                        lib.path,
                        if lib.is_public { "yes" } else { "no" }
                    );
                }
            }
        }

        LibraryCommand::Scan { all, name } => {
            let libraries = if all {
                db.list_libraries()?
            } else if let Some(name) = name {
                db.get_library_by_name(&name)?
                    .map(|l| vec![l])
                    .unwrap_or_default()
            } else {
                db.list_libraries()?
            };

            if libraries.is_empty() {
                println!("No libraries to scan.");
                return Ok(());
            }

            for lib in libraries {
                println!("Scanning library: {} ({})", lib.name, lib.path);
                println!("  (Use 'ebook-rs serve' to scan libraries automatically)");
            }
        }
    }

    Ok(())
}

/// Start the server.
async fn cmd_serve(
    mut config: Config,
    bind: Option<std::net::SocketAddr>,
    library: Option<PathBuf>,
) -> anyhow::Result<()> {
    // Override bind address if specified
    if let Some(addr) = bind {
        config.server.bind = addr;
    }

    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ebook_rs=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Open database
    let db = Database::open(&config.database.path)?;

    // Create auth service
    let auth = AuthService::new(
        db.clone(),
        config.auth.session_days,
        config.auth.registration_enabled(),
    );

    tracing::info!(
        bind = %config.server.bind,
        database = %config.database.path.display(),
        "Starting ebook-rs server"
    );

    // Get libraries from database or legacy mode
    let libraries = db.list_libraries()?;

    if libraries.is_empty() {
        if let Some(lib_path) = library {
            // Legacy mode: single library from CLI
            tracing::info!(
                "Running in legacy mode with library: {}",
                lib_path.display()
            );

            // Create a temporary library entry
            let library = ebook_rs::db::Library {
                id: uuid::Uuid::new_v4().to_string(),
                name: "default".to_string(),
                path: lib_path.to_string_lossy().to_string(),
                is_public: true,
                owner_id: None,
                created_at: ebook_rs::db::now_timestamp(),
            };
            db.create_library(&library)?;
        } else {
            tracing::warn!(
                "No libraries configured. Add one with: ebook-rs library add <name> --path /path/to/books"
            );
        }
    }

    // Create application state
    let state = server::AppState::new_with_db(config.clone(), db.clone(), auth);

    // Step 1: Load from database (instant startup)
    tracing::info!("Loading library from database...");
    if let Err(e) = state.load_from_db() {
        tracing::warn!(error = %e, "Failed to load from database, will scan");
    }

    // Step 2: Start background scan (non-blocking)
    tracing::info!("Starting background library scan...");
    state.start_background_scan();

    // Start background rescan task if enabled
    if config.scan.interval_seconds > 0 {
        let state_clone = state.clone();
        let interval = Duration::from_secs(config.scan.interval_seconds);

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // Skip first immediate tick

            loop {
                ticker.tick().await;
                tracing::debug!("Running scheduled library rescan");

                if let Err(e) = state_clone.scan_all_libraries() {
                    tracing::warn!(error = %e, "Scheduled rescan failed");
                }
            }
        });
    }

    // Create router
    let app = server::create_router(state);

    // Start server IMMEDIATELY (don't wait for scan)
    let listener = TcpListener::bind(config.server.bind).await?;
    tracing::info!(address = %config.server.bind, "Server listening (background scan in progress)");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Prompt for password input.
fn prompt_password(prompt: &str) -> anyhow::Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;

    let mut password = String::new();
    io::stdin().read_line(&mut password)?;

    Ok(password.trim().to_string())
}
