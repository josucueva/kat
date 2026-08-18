use clap::{Parser, Subcommand};

pub mod machine_presenter;

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
    long_about = "A specification-first semantic software repository for representing, evolving, tracing, and validating authoritative software knowledge independently from source code files.",
    help_template = "\
{about-with-newline}
Usage: {usage}

Everyday workflow:
  status     Show accepted repository state and current draft status
  context    Retrieve bounded semantic development context around elements
  author     Stage a semantic Change from declarative JSON
  check      Check consistency, evidence, accountability, and graph quality
  commit     Publish the current draft Change
  abort      Discard the current draft Change

Inspection:
  list       List knowledge elements in the accepted state
  show       Inspect a resolved knowledge element
  history    Show accepted Change history
  trace      Trace an element to its semantic origin
  impact     Analyze consequences of changing an element
  artifacts  Inspect artifact accountability
  ontology   Discover semantic types and valid relationships
  validate   Run mechanical consistency validation

Advanced authoring:
  create     Create a knowledge element
  update     Update an active knowledge element
  deprecate  Deprecate an active knowledge element
  supersede  Replace an element while preserving semantic history
  link       Establish a semantic relationship
  unlink     Remove a semantic relationship
  account    Re-baseline artifact accountability
  change     Manage draft Change transactions explicitly

Repository:
  init       Initialize a KAT repository

Options:
{options}
"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    // -----------------------------------------------------------------------
    // Everyday Workflow
    // -----------------------------------------------------------------------
    /// Show accepted repository state and current draft status
    ///
    /// Displays where the repository is right now: current accepted state summary
    /// plus active draft transaction status.
    ///
    /// Machine JSON supported: --json flag.
    #[command(next_help_heading = "Everyday Workflow")]
    Status {
        /// Display compact single-line dashboard
        #[arg(long)]
        compact: bool,

        /// Output structured machine JSON envelope
        #[arg(long)]
        json: bool,
    },

    /// Retrieve bounded semantic development context around elements
    ///
    /// Context is a deterministic semantic projection over accepted state.
    /// It is intended for orientation and development routing.
    ///
    /// It does not claim to return every physical file involved in a change.
    ///
    /// Machine JSON supported: --json flag.
    #[command(next_help_heading = "Everyday Workflow")]
    Context {
        /// Root element references (UUIDs, 8-hex prefixes, or @handles)
        roots: Vec<String>,

        /// Traversal direction (upstream, downstream, both)
        #[arg(long, default_value = "upstream")]
        direction: String,

        /// Maximum depth of relationship hops
        #[arg(long)]
        depth: Option<usize>,

        /// Group context elements by ontology category
        #[arg(long)]
        categorize: bool,

        /// Output structured machine JSON envelope
        #[arg(long)]
        json: bool,
    },

    /// Stage a semantic Change from declarative JSON
    ///
    /// Accepts a JSON array of AuthorClaim objects via CLAIMS_FILE or stdin (-).
    ///
    /// AUTHORING CLAIMS:
    ///   create_element     Create a new knowledge element
    ///   update_element     Update title/description of an existing active element
    ///   deprecate_element  Deprecate an active knowledge element
    ///   supersede_element  Supersede an existing element with a replacement element
    ///   link_element       Establish a semantic relationship between elements
    ///   unlink_element     Remove a relationship from draft state
    ///   account_artifact   Re-baseline artifact accountability
    ///
    /// WORKFLOW REFERENCES:
    ///   A create_element claim may declare a temporary handle such as "@req-auth".
    ///   Later claims in the same draft Change may use that handle instead of a UUID.
    ///
    ///   WORKFLOW REFERENCES ARE DRAFT-LOCAL. They exist only during the active draft
    ///   Change and EXPIRE when the Change is committed or aborted.
    ///
    /// CROSS-CHANGE REFERENCES:
    ///   To reference an element from a previously accepted Change, use its stable
    ///   UUID or unambiguous UUID prefix. Handles do not persist across Changes.
    ///
    /// MULTI-CHANGE AUTHORING:
    ///   Change 1:
    ///     kat author requirements.json
    ///     kat check
    ///     kat commit
    ///
    ///   Change 2:
    ///     kat author architecture.json
    ///     kat check
    ///     kat commit
    ///
    ///   References declared with @handles in requirements.json expire after the
    ///   first commit. Later Changes reference accepted elements using stable UUIDs.
    ///
    /// ONTOLOGY GUIDANCE:
    ///   If a relationship claim violates endpoint constraints, KAT reports the
    ///   provided source/target types together with allowed source and target types.
    ///
    /// BEHAVIOR:
    ///   - Valid input is staged atomically into open draft transaction.
    ///   - Invalid non-empty input stages 0 operations and leaves draft unchanged.
    ///   - Empty/whitespace input is a successful no-op (0 claims processed).
    ///   - If no draft Change exists, one is created automatically.
    ///
    /// MACHINE JSON SUPPORT:
    ///   Supported via --json flag (code AUTHOR_PARSE_ERROR / AUTHOR_COMPILATION_FAILED).
    ///
    /// SEE ALSO:
    ///   kat author --example, kat status, kat check, kat commit, kat abort, kat ontology
    #[command(next_help_heading = "Everyday Workflow")]
    Author {
        /// Path to JSON file containing declarative claims (reads stdin if omitted or '-')
        claims_file: Option<String>,

        /// Print a complete working JSON example and exit
        #[arg(short = 'e', long)]
        example: bool,

        /// Output structured machine JSON envelope
        #[arg(long)]
        json: bool,
    },

    /// Check consistency, evidence, accountability, and graph quality
    ///
    /// Evaluates whether the repository is healthy: mechanical consistency,
    /// evidence coverage, artifact accountability baselines, and graph quality advisories.
    ///
    /// Machine JSON supported: --json flag.
    #[command(next_help_heading = "Everyday Workflow")]
    Check {
        /// Output structured machine JSON envelope
        #[arg(long)]
        json: bool,

        /// Display compact single-line summary
        #[arg(long)]
        compact: bool,
    },

    /// Publish the current draft Change
    ///
    /// Commits all staged operations in open draft transaction into a single
    /// ChangeRevision and publishes to accepted state.
    ///
    /// Machine JSON supported: --json flag.
    #[command(next_help_heading = "Everyday Workflow")]
    Commit {
        /// Output structured machine JSON envelope
        #[arg(long)]
        json: bool,
    },

    /// Discard the current draft Change
    ///
    /// Aborts open transaction and discards all staged operations.
    ///
    /// Machine JSON supported: --json flag.
    #[command(next_help_heading = "Everyday Workflow")]
    Abort {
        /// Output structured machine JSON envelope
        #[arg(long)]
        json: bool,
    },

    // -----------------------------------------------------------------------
    // Inspection
    // -----------------------------------------------------------------------
    /// List knowledge elements in the current accepted state
    #[command(next_help_heading = "Inspection")]
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

    /// Inspect a resolved knowledge element
    #[command(next_help_heading = "Inspection")]
    Show {
        /// Element ID (UUID or prefix) to display
        element_id: String,

        /// Display compact single-line element summary
        #[arg(long)]
        compact: bool,
    },

    /// Show accepted Change history
    #[command(next_help_heading = "Inspection")]
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

    /// Trace an element to its semantic origin
    #[command(next_help_heading = "Inspection")]
    Trace {
        /// Target element ID (UUID or prefix) to trace
        element_id: String,

        /// Display explicit exhaustive path list instead of collapsed tree hierarchy
        #[arg(long)]
        paths: bool,

        /// Limit traversal depth to N relationship hops
        #[arg(long)]
        max_depth: Option<usize>,

        /// Display compact arrow-joined path rendering
        #[arg(long)]
        compact: bool,
    },

    /// Analyze consequences of changing an element
    #[command(next_help_heading = "Inspection")]
    Impact {
        /// Target element ID (UUID or prefix) to analyze
        element_id: String,

        /// Limit impact propagation depth to N relationship hops
        #[arg(long)]
        max_depth: Option<usize>,

        /// Display compact flat table layout
        #[arg(long)]
        compact: bool,
    },

    /// Evaluate artifact accountability baselines
    #[command(next_help_heading = "Inspection")]
    Artifacts {
        /// Filter accountability report to display only STALE artifacts
        #[arg(long)]
        stale: bool,

        /// Optional artifact element ID (UUID or prefix) for detailed inspection
        artifact_id: Option<String>,

        /// Display compact status table layout
        #[arg(long)]
        compact: bool,
    },

    /// Discover semantic types and valid relationships
    #[command(next_help_heading = "Inspection")]
    Ontology {
        /// Display compact shortened type IDs without human-readable names
        #[arg(long, global = true)]
        compact: bool,

        #[command(subcommand)]
        command: Option<OntologyCommands>,
    },

    /// Run mechanical consistency validation
    ///
    /// Runs lower-level mechanical validation checks directly against accepted state.
    /// For comprehensive quality and evidence checking, use kat check.
    #[command(next_help_heading = "Inspection")]
    Validate {
        /// Focus on validation evidence coverage reporting across knowledge categories
        #[arg(long)]
        coverage: bool,

        /// Display compact single-line counts summary
        #[arg(long)]
        compact: bool,
    },

    // -----------------------------------------------------------------------
    // Advanced Authoring
    // -----------------------------------------------------------------------
    /// Create a knowledge element (prefer kat author for normal authoring)
    #[command(next_help_heading = "Advanced Authoring")]
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

    /// Update an active knowledge element (prefer kat author for normal authoring)
    #[command(next_help_heading = "Advanced Authoring")]
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

    /// Deprecate an active knowledge element (prefer kat author for normal authoring)
    #[command(next_help_heading = "Advanced Authoring")]
    Deprecate {
        /// Element ID (UUID) to deprecate
        element_id: String,
    },

    /// Replace an element while preserving semantic history (prefer kat author for normal authoring)
    #[command(next_help_heading = "Advanced Authoring")]
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

    /// Establish a semantic relationship (prefer kat author for normal authoring)
    ///
    /// If a relationship violates ontology endpoint constraints, KAT reports the
    /// provided source/target types together with allowed source and target types.
    #[command(next_help_heading = "Advanced Authoring")]
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

    /// Remove a semantic relationship (prefer kat author for normal authoring)
    #[command(next_help_heading = "Advanced Authoring")]
    Unlink {
        /// Relationship ID (UUID) to remove
        relationship_id: String,

        /// Optional detailed description
        #[arg(long)]
        description: Option<String>,
    },

    /// Re-baseline artifact accountability (prefer kat author for normal authoring)
    #[command(next_help_heading = "Advanced Authoring")]
    Account {
        /// Artifact element ID or unique hex prefix
        artifact_id: String,

        /// Optional detailed description
        #[arg(long)]
        description: Option<String>,
    },

    /// Manage draft Change transactions explicitly
    ///
    /// For normal authoring, kat-author(1) automatically opens a draft Change
    /// when needed. Use kat change when explicit transaction control is required.
    #[command(next_help_heading = "Advanced Authoring")]
    Change {
        #[command(subcommand)]
        command: ChangeCommands,
    },

    // -----------------------------------------------------------------------
    // Repository
    // -----------------------------------------------------------------------
    /// Initialize a KAT repository
    #[command(next_help_heading = "Repository")]
    Init,
}

#[derive(Subcommand, Debug)]
pub enum OntologyCommands {
    /// Inspect detailed capabilities and endpoint admissibility for a type
    Show {
        /// Type ID (e.g. `kat.core/requirement` or `requirement`)
        type_id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum ChangeCommands {
    /// Open a new multi-operation change transaction
    Begin {
        /// Optional change description
        #[arg(long)]
        description: Option<String>,

        /// Output structured machine JSON envelope
        #[arg(long)]
        json: bool,
    },
    /// Inspect status and staged operations of the open change transaction
    Status {
        /// Display compact summary
        #[arg(long)]
        compact: bool,

        /// Output structured machine JSON envelope
        #[arg(long)]
        json: bool,
    },
    /// Commit all staged operations into a single ChangeRevision and publish
    Commit {
        /// Output structured machine JSON envelope
        #[arg(long)]
        json: bool,
    },
    /// Abort the open change transaction and discard all staged operations
    Abort {
        /// Output structured machine JSON envelope
        #[arg(long)]
        json: bool,
    },
}
