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
    /// Search cards with Anki's search syntax
    Search {
        /// e.g. `deck:Korean is:due tag:leech`
        query: Vec<String>,
        /// field | deck | due | interval | ease | reps | lapses | created | modified | tags | notetype
        #[arg(long, default_value = "field")]
        sort: String,
        #[arg(long)]
        reverse: bool,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Show everything about one card (stats, FSRS state, review log)
    Card { card_id: i64 },
    /// Apply an operation to every card matching a search
    Bulk {
        query: Vec<String>,
        #[arg(long, group = "op")]
        suspend: bool,
        #[arg(long, group = "op")]
        unsuspend: bool,
        #[arg(long, group = "op")]
        bury: bool,
        /// 0 clears; 1-7 = red orange green blue pink turquoise purple
        #[arg(long, group = "op", value_name = "N")]
        flag: Option<u8>,
        /// Space-separated tags to add
        #[arg(long, group = "op", value_name = "TAGS")]
        tag: Option<String>,
        /// Space-separated tags to remove
        #[arg(long, group = "op", value_name = "TAGS")]
        untag: Option<String>,
        /// Move cards to a deck (created if missing)
        #[arg(long = "move", group = "op", value_name = "DECK")]
        move_to: Option<String>,
        /// Reset to new, keeping history
        #[arg(long, group = "op")]
        forget: bool,
        /// Anki due spec: 0, 3, 1-7, 2!
        #[arg(long, group = "op", value_name = "DAYS")]
        due: Option<String>,
        /// Delete the matching notes and all their cards
        #[arg(long, group = "op")]
        delete: bool,
        /// Skip the confirmation for --delete
        #[arg(long)]
        yes: bool,
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
    // `ankh search … | head` must not panic on a closed pipe.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    let paths = ankh_core::Paths::resolve(cli.profile.as_deref(), cli.collection.as_deref());
    cli::init_logging(&paths);
    let code = match cli.command.unwrap_or(Command::Tui) {
        Command::Tui => tui::run(paths),
        cmd => cli::run(cmd, paths, cli.format),
    };
    std::process::exit(code);
}
