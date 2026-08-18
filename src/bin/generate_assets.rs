use std::fs;
use std::io::Error;
use std::path::Path;

use clap::CommandFactory;
use clap_complete::{Shell, generate_to};
use clap_mangen::Man;

use kat::cli::Cli;

fn main() -> Result<(), Error> {
    let man_dir = Path::new("generated/man");
    let comp_dir = Path::new("generated/completions");

    fs::create_dir_all(man_dir)?;
    fs::create_dir_all(comp_dir)?;

    let mut cmd = Cli::command();
    cmd.set_bin_name("kat");

    // 1. Generate man pages
    println!("Generating UNIX man pages in generated/man/...");
    render_man_pages(&cmd, man_dir)?;

    // 2. Generate shell completions
    println!("Generating shell completions in generated/completions/...");
    let mut cmd_for_comp = cmd.clone();
    generate_to(Shell::Bash, &mut cmd_for_comp, "kat", comp_dir)?;
    generate_to(Shell::Zsh, &mut cmd_for_comp, "kat", comp_dir)?;
    generate_to(Shell::Fish, &mut cmd_for_comp, "kat", comp_dir)?;

    println!("Asset generation completed successfully!");
    Ok(())
}

fn render_man_pages(cmd: &clap::Command, out_dir: &Path) -> Result<(), Error> {
    // 1. Generate kat(1) root man page
    let root_man = r#".TH kat 1 "kat 0.4.1"
.SH NAME
kat \- Knowledge Abstraction Tracker
.SH SYNOPSIS
.B kat
[\fB\-h\fR|\fB\-\-help\fR] [\fB\-V\fR|\fB\-\-version\fR] <\fIcommand\fR>
.SH DESCRIPTION
A specification\-first semantic software repository for representing, evolving, tracing, and validating authoritative software knowledge independently from source code files.
.SH EVERYDAY WORKFLOW
.TP
.BR kat\-status (1)
Show accepted repository state and current draft status
.TP
.BR kat\-context (1)
Retrieve bounded semantic development context around elements
.TP
.BR kat\-author (1)
Stage a semantic Change from declarative JSON
.TP
.BR kat\-check (1)
Check consistency, evidence, accountability, and graph quality
.TP
.BR kat\-commit (1)
Publish the current draft Change
.TP
.BR kat\-abort (1)
Discard the current draft Change
.SH INSPECTION
.TP
.BR kat\-list (1)
List knowledge elements in the accepted state
.TP
.BR kat\-show (1)
Inspect a detailed view of a resolved knowledge element
.TP
.BR kat\-history (1)
Show accepted Change history
.TP
.BR kat\-trace (1)
Trace an element to its semantic origin
.TP
.BR kat\-impact (1)
Analyze consequences of changing an element
.TP
.BR kat\-artifacts (1)
Evaluate artifact accountability baselines
.TP
.BR kat\-ontology (1)
Discover semantic types and valid relationships
.TP
.BR kat\-validate (1)
Run mechanical consistency validation
.SH ADVANCED AUTHORING
.TP
.BR kat\-create (1)
Create a knowledge element (prefer kat-author(1) for normal authoring)
.TP
.BR kat\-update (1)
Update an active knowledge element (prefer kat-author(1) for normal authoring)
.TP
.BR kat\-deprecate (1)
Deprecate an active knowledge element (prefer kat-author(1) for normal authoring)
.TP
.BR kat\-supersede (1)
Replace an element while preserving semantic history (prefer kat-author(1) for normal authoring)
.TP
.BR kat\-link (1)
Establish a semantic relationship (prefer kat-author(1) for normal authoring)
.TP
.BR kat\-unlink (1)
Remove a semantic relationship (prefer kat-author(1) for normal authoring)
.TP
.BR kat\-account (1)
Re-baseline artifact accountability (prefer kat-author(1) for normal authoring)
.TP
.BR kat\-change (1)
Manage draft Change transactions explicitly
.SH REPOSITORY
.TP
.BR kat\-init (1)
Initialize a KAT repository
.SH BASIC WORKFLOW
.nf
Initialize:
    kat init

Inspect state:
    kat status

Stage a semantic Change:
    kat author change.json

Check health:
    kat check

Publish Change:
    kat commit

Retrieve development context:
    kat context <element>
.fi
.SH OPTIONS
.TP
\fB\-h\fR, \fB\-\-help\fR
Print help
.TP
\fB\-V\fR, \fB\-\-version\fR
Print version
.SH SEE ALSO
.BR kat\-author (1),
.BR kat\-status (1),
.BR kat\-check (1),
.BR kat\-context (1)
"#;
    fs::write(out_dir.join("kat.1"), root_man)?;

    // 2. Generate kat-author(1) canonical authoring reference man page
    let author_man = r#".TH kat\-author 1 "kat 0.4.1"
.SH NAME
kat\-author \- Stage a semantic Change from declarative JSON
.SH SYNOPSIS
.B kat author
[\fB\-e\fR|\fB\-\-example\fR] [\fB\-\-json\fR] [\fICLAIMS_FILE\fR]
.SH DESCRIPTION
Declaratively compile and stage a JSON batch of authoring claims into the active draft transaction. If no draft Change exists, one is opened automatically.
.SH INPUT FORMAT
Accepts a JSON array of claim objects via \fICLAIMS_FILE\fR or standard input (\fB\-\fR).
.SH CLAIM KINDS
.TP
.B create_element
Declare a new knowledge element (requires \fBtype_id\fR, \fBtitle\fR; optional \fBdescription\fR, \fBhandle\fR).
.TP
.B update_element
Patch title or description on an active element.
.TP
.B deprecate_element
Mark an active element as Deprecated.
.TP
.B supersede_element
Replace an existing element while preserving historical linkage.
.TP
.B link_element
Establish a typed semantic relationship between two elements.
.TP
.B unlink_element
Remove an existing relationship from draft state.
.TP
.B account_artifact
Re-baseline artifact accountability.
.SH WORKFLOW REFERENCES
A \fBcreate_element\fR claim may declare a temporary handle such as \fB"@req-auth"\fR.

Later claims within the same draft Change may use that handle in place of a UUID reference.

\fBWorkflow references are draft-local.\fR They exist only for the duration of the active draft Change and expire when the Change is published (\fBcommit\fR) or discarded (\fBabort\fR). Forward references within a single claim batch are not allowed.
.SH CROSS-CHANGE REFERENCES
Elements established in previously accepted Changes must be referenced using their stable UUIDs or unambiguous UUID prefixes. Workflow handles do not persist across Change boundaries.
.SH ATOMICITY
Valid input is compiled and staged atomically. If any claim in a non-empty document is invalid, the entire input is rejected with exit code 1 and 0 operations are staged, leaving any pre-existing draft transaction byte-for-byte unchanged. Empty or whitespace-only input is a successful no-op (exit 0).
.SH EXAMPLES
.nf
[
  {
    "kind": "create_element",
    "type_id": "kat.core/requirement",
    "title": "Auth Spec",
    "handle": "@req-auth"
  },
  {
    "kind": "create_element",
    "type_id": "kat.core/implementation",
    "title": "Auth Service",
    "handle": "@imp-auth"
  },
  {
    "kind": "link_element",
    "relationship_type_id": "kat.core/realizes",
    "source_ref": "@imp-auth",
    "target_ref": "@req-auth"
  }
]
.fi
.SH ERRORS
If input cannot be parsed or violates ontology rules, KAT outputs explicit error information. In machine mode (\fB\-\-json\fR), error code \fBAUTHOR_PARSE_ERROR\fR or \fBAUTHOR_COMPILATION_FAILED\fR is returned with structured line, column, and reason details. Endpoint constraint violations return allowed source/target element types.
.SH EXIT STATUS
.TP
.B 0
Success (claims staged or empty input no-op).
.TP
.B 1
Parse failure, compilation failure, or repository error (0 claims staged).
.TP
.B 2
Invalid CLI option combination (e.g. \fB\-\-example\fR with \fICLAIMS_FILE\fR or \fB\-\-json\fR).
.SH MACHINE OUTPUT
Supported via \fB\-\-json\fR flag, emitting structured \fBCommonResultEnvelope\fR.
.SH SEE ALSO
.BR kat (1),
.BR kat\-status (1),
.BR kat\-check (1),
.BR kat\-commit (1),
.BR kat\-abort (1),
.BR kat\-ontology (1)
"#;
    fs::write(out_dir.join("kat-author.1"), author_man)?;

    // 3. Generate remaining subcommand man pages
    for sub in cmd.get_subcommands() {
        let sub_name = sub.get_name();
        if sub_name == "help" || sub_name == "author" {
            continue;
        }
        let sub_man = Man::new(sub.clone());
        let mut sub_buf = Vec::new();
        sub_man.render(&mut sub_buf)?;
        let sub_path = out_dir.join(format!("kat-{sub_name}.1"));
        fs::write(&sub_path, sub_buf)?;
    }

    Ok(())
}
