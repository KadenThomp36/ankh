mod cli;
mod tui;

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "ankh", version, about = "A neovim-flavoured Anki client for the terminal", long_about = None)]
pub struct Cli {
    /// Profile name (each profile has its own collection and login)
    #[arg(long, global = true, env = "ANKH_PROFILE")]
    profile: Option<String>,

    /// Use an existing collection.anki2 instead of the profile's own
    #[arg(long, global = true, env = "ANKH_COLLECTION", value_name = "PATH")]
    collection: Option<PathBuf>,

    /// Output format for headless commands
    #[arg(long, global = true, value_enum, default_value_t = Format::Table)]
    format: Format,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Table,
    Json,
    Jsonl,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Open the full-screen interface (default when no command is given)
    Tui,
    /// Log in to AnkiWeb and store the sync key in the OS keyring
    Login {
        /// AnkiWeb username (email); prompted if omitted
        #[arg(long, short)]
        username: Option<String>,
        /// Read the password from stdin instead of prompting
        #[arg(long)]
        password_stdin: bool,
    },
    /// Forget the stored sync key
    Logout,
    /// Show login and collection status
    Status,
    /// Sync with AnkiWeb
    Sync {
        /// Resolve a full-sync conflict by downloading AnkiWeb's copy
        #[arg(long, conflicts_with = "upload")]
        download: bool,
        /// Resolve a full-sync conflict by uploading the local copy
        #[arg(long, conflicts_with = "download")]
        upload: bool,
        /// Skip media sync
        #[arg(long)]
        no_media: bool,
        /// Sync media only
        #[arg(long, conflicts_with_all = ["download", "upload", "no_media"])]
        media_only: bool,
    },
    /// List decks with due counts
    Decks,
    /// Print the next due card of a deck (question, answer, buttons)
    Next {
        /// Deck name (`Korean::Vocab`); defaults to the current deck
        deck: Option<String>,
    },
    /// Answer a card previously shown by `next`
    Answer {
        card_id: i64,
        /// again | hard | good | easy, or 1-4
        rating: String,
        /// Seconds spent, recorded in the review log
        #[arg(long, default_value_t = 10)]
        secs: u32,
    },
}

fn main() {
    let cli = Cli::parse();
    let paths = ankh_core::Paths::resolve(cli.profile.as_deref(), cli.collection.as_deref());
    cli::init_logging(&paths);
    let code = match cli.command.unwrap_or(Command::Tui) {
        Command::Tui => tui::run(paths),
        cmd => cli::run(cmd, paths, cli.format),
    };
    std::process::exit(code);
}
