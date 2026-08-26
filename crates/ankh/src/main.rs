mod cli;
mod editor;
mod lua;
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
    /// Print a note as an ankh note file (Markdown + frontmatter)
    Note { note_id: i64 },
    /// Edit a note in $EDITOR
    Edit { note_id: i64 },
    /// Add notes: from fields on the command line, a note file, or $EDITOR
    Add {
        /// Field values in notetype order (omit to open $EDITOR)
        fields: Vec<String>,
        #[arg(long, short)]
        deck: Option<String>,
        #[arg(long, short)]
        notetype: Option<String>,
        /// Space-separated tags
        #[arg(long, short)]
        tags: Option<String>,
        /// Read a note file (`-` for stdin)
        #[arg(long, short, value_name = "PATH")]
        file: Option<String>,
    },
    /// Export notes as a note file (Markdown), or an .apkg with --apkg
    Export {
        /// Anki search; default: everything
        query: Vec<String>,
        /// Write to a file instead of stdout
        #[arg(long, short, value_name = "PATH")]
        out: Option<PathBuf>,
        /// Write an Anki package instead of Markdown (requires --out)
        #[arg(long)]
        apkg: bool,
        /// Include scheduling information in the .apkg
        #[arg(long)]
        with_scheduling: bool,
        /// Leave media out of the .apkg
        #[arg(long)]
        no_media: bool,
    },
    /// Import a note file (.md), an Anki package (.apkg) or a CSV/TSV
    Import {
        path: PathBuf,
        /// CSV only: notetype whose fields the columns map to, in order
        #[arg(long, short)]
        notetype: Option<String>,
        /// CSV only: deck to import into
        #[arg(long, short)]
        deck: Option<String>,
    },
    /// Create, rename or delete decks
    Deck {
        #[command(subcommand)]
        op: DeckOp,
    },
    /// Show or edit a deck's options preset (TOML)
    Options {
        deck: String,
        /// Open the options in $EDITOR
        #[arg(long, short)]
        edit: bool,
        /// Turn FSRS on or off for the collection
        #[arg(long, value_name = "on|off")]
        fsrs: Option<String>,
    },
    /// Optimise FSRS parameters for a deck's preset from its review history
    Fsrs {
        #[command(subcommand)]
        op: FsrsOp,
    },
    /// Statistics for a deck (or the whole collection)
    Stats {
        deck: Option<String>,
        /// Days of history for per-day series
        #[arg(long, default_value_t = 365)]
        days: u32,
    },
    /// List notetypes and their fields
    Notetypes,
    /// Show config paths, or dump the built-in defaults.lua
    Config {
        /// Print the embedded defaults.lua (copy it to init.lua to customise)
        #[arg(long)]
        defaults: bool,
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

#[derive(Subcommand, Debug)]
pub enum DeckOp {
    /// Create a deck (`Parent::Child` nests)
    Create { name: String },
    /// Rename a deck (moves it if the new name has a different parent)
    Rename { name: String, new_name: String },
    /// Delete a deck, its subdecks and all their cards
    Delete {
        name: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum FsrsOp {
    /// Compute and store optimal parameters for the deck's preset
    Optimize { deck: String },
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
