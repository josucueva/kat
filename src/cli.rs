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
        /// Element ID (UUID) to display
        element_id: String,
    },

    /// Reconstruct and display the accepted change revision graph
    History,

    /// Trace a knowledge element back to its origin
    Trace {
        /// Target element ID (UUID) to trace
        element_id: String,
    },

    /// Analyze potential impact and consequences of changing an element
    Impact {
        /// Target element ID (UUID) to analyze
        element_id: String,
    },

    /// Run mechanical consistency validation across the current accepted state
    Validate,

    /// Evaluate artifact accountability baselines against accepted state
    Artifacts,
}
