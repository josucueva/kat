use clap::{Parser, Subcommand};

/// KAT (Knowledge Abstraction Tracker)
///
/// A specification-first semantic software repository for representing,
/// evolving, tracing, and validating software as knowledge.
#[derive(Parser, Debug)]
#[command(
    name = "kat",
    author,
    version,
    about = "Knowledge Abstraction Tracker",
    long_about = "A specification-first semantic software repository for representing, evolving, tracing, and validating authoritative software knowledge independently from source code files."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Initialize a new KAT repository in the current directory
    Init,

    /// Display a concise summary of current accepted repository status and health
    Status {
        /// Display compact single-line dashboard
        #[arg(long)]
        compact: bool,
    },

    /// List knowledge elements in the current accepted state
    List {
        /// Optional element type positional shorthand (e.g. requirement, design-decision)
        element_type: Option<String>,

        /// Filter by element type (e.g. requirement, design-decision)
        #[arg(long = "type")]
        type_flag: Option<String>,

        /// Filter by lifecycle state (active, deprecated, superseded)
        #[arg(long)]
        lifecycle: Option<String>,
    },

    /// Create a new knowledge element
    Create {
        /// Type of element to create (e.g. requirement, constraint, design-decision, implementation, artifact)
        element_type: String,

        /// Title of the knowledge element
        #[arg(long)]
        title: String,

        /// Optional detailed description
        #[arg(long)]
        description: Option<String>,
    },

    /// Update title or description of an existing active knowledge element
    Update {
        /// Element ID (UUID) to update
        element_id: String,

        /// New title for the element
        #[arg(long)]
        title: Option<String>,

        /// New description for the element
        #[arg(long)]
        description: Option<String>,
    },

    /// Mark an active knowledge element as Deprecated
    Deprecate {
        /// Element ID (UUID) to deprecate
        element_id: String,
    },

    /// Supersede an existing knowledge element with a new replacement element
    Supersede {
        /// Existing element ID (UUID) to supersede
        existing_element_id: String,

        /// Replacement element type (e.g. requirement, design-decision)
        replacement_type: String,

        /// Title for the replacement element
        #[arg(long)]
        title: String,

        /// Optional detailed description for the replacement element
        #[arg(long)]
        description: Option<String>,
    },

    /// Establish a semantic relationship between two elements
    Link {
        /// Relationship type (e.g. addresses, depends-on, derived-from, guides, motivates, realizes, represents, restricts, validates)
        relationship_type: String,

        /// Source element ID (UUID)
        source_element_id: String,

        /// Target element ID (UUID)
        target_element_id: String,

        /// Optional detailed description
        #[arg(long)]
        description: Option<String>,
    },

    /// Remove a relationship from the current accepted state
    Unlink {
        /// Relationship ID (UUID) to remove
        relationship_id: String,

        /// Optional detailed description
        #[arg(long)]
        description: Option<String>,
    },

    /// Show detailed view of a resolved active knowledge element
    Show {
        /// Element ID (UUID or prefix) to display
        element_id: String,

        /// Display compact single-line element summary
        #[arg(long)]
        compact: bool,
    },

    /// Reconstruct and display the accepted change revision graph
    History {
        /// Format each history entry as a single line
        #[arg(long)]
        oneline: bool,

        /// Limit output to the N most recent revisions
        #[arg(long)]
        limit: Option<usize>,

        /// Filter history to revisions touching a specific element ID or prefix
        #[arg(long)]
        element: Option<String>,

        /// Display compact output
        #[arg(long)]
        compact: bool,
    },

    /// Trace a knowledge element back to its origin
    Trace {
        /// Target element ID (UUID or prefix) to trace
        element_id: String,

        /// Display compact arrow-joined path rendering
        #[arg(long)]
        compact: bool,
    },

    /// Analyze potential impact and consequences of changing an element
    Impact {
        /// Target element ID (UUID or prefix) to analyze
        element_id: String,

        /// Display compact flat table layout
        #[arg(long)]
        compact: bool,
    },

    /// Run mechanical consistency validation across the current accepted state
    Validate {
        /// Display compact single-line counts summary
        #[arg(long)]
        compact: bool,
    },

    /// Evaluate artifact accountability baselines against accepted state
    Artifacts {
        /// Display compact status table layout
        #[arg(long)]
        compact: bool,
    },
}
