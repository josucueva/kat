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
    let root_man = r#".TH kat 1 "kat 0.4.2"
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
Inspect a resolved knowledge element
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
Inspect artifact accountability
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

Review repository health:
    kat check

Publish:
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
    let author_man = r#".TH kat\-author 1 "kat 0.4.2"
.SH NAME
kat\-author \- Stage a semantic Change from declarative JSON
.SH SYNOPSIS
.B kat author
[\fB\-e\fR|\fB\-\-example\fR] [\fB\-\-json\fR] [\fICLAIMS_FILE\fR]
.SH DESCRIPTION
Declaratively compile and stage a JSON batch of authoring claims into the active draft transaction. If no draft Change exists, one is opened automatically.
.SH INPUT FORMAT
Accepts a JSON array of claim objects via \fICLAIMS_FILE\fR or standard input (\fB\-\fR).

The normative JSON format uses internal tagging with a \fBkind\fR discriminator field (snake_case):
.nf
[
  {
    "kind": "create_element",
    "type_id": "kat.core/requirement",
    "title": "Auth Spec",
    "handle": "@req-auth"
  }
]
.fi

Legacy v0.4.0 externally tagged DTOs (e.g. \fB{"CreateElement": {...}}\fR) remain supported for backward compatibility, but internal tagging with \fBkind\fR is normative.
.SH CLAIM KINDS
.TP
.B create_element
Declare a new knowledge element (requires \fBtype_id\fR / \fBtype\fR, \fBtitle\fR; optional \fBdescription\fR, \fBhandle\fR).
.TP
.B update_element
Patch title or description on an active element (requires \fBelement_id\fR; optional \fBtitle\fR, \fBdescription\fR).
.TP
.B deprecate_element
Mark an active element as Deprecated (requires \fBelement_id\fR).
.TP
.B supersede_element
Replace an existing element while preserving historical linkage (requires \fBexisting_element_id\fR, \fBreplacement_type\fR, \fBtitle\fR; optional \fBdescription\fR, \fBhandle\fR).
.TP
.B link_element
Establish a typed semantic relationship between two elements (requires \fBrelationship_type_id\fR / \fBrelationship_type\fR, \fBsource_ref\fR, \fBtarget_ref\fR; optional \fBdescription\fR).
.TP
.B unlink_element
Remove an existing relationship from draft state (requires \fBrelationship_id\fR; optional \fBdescription\fR).
.TP
.B account_artifact
Re-baseline artifact accountability (requires \fBartifact_id\fR; optional \fBdescription\fR).
.SH WORKFLOW REFERENCES
@handles are temporary references scoped to the current draft Change.
They may be used by later claims in that Change after the defining
claim has succeeded.

Workflow references expire when the Change is committed or aborted. Duplicate handles within the same batch fail compilation atomically. Forward references (referencing a handle before its declaration) are rejected.
.SH CROSS-CHANGE REFERENCES
To reference knowledge from an accepted Change, use its stable UUID
or an unambiguous UUID prefix. Workflow references are not persistent
aliases.
.SH ATOMICITY
Valid input is compiled and staged atomically. If any claim in a non-empty document is invalid, the entire input is rejected with exit code 1 and 0 operations are staged, leaving any pre-existing draft transaction byte-for-byte unchanged.
.SH EMPTY INPUT BEHAVIOR
Empty or whitespace-only input is a successful no-op (0 claims processed, exit status 0). If no draft Change exists, no draft transaction is opened.
.SH INFORMATIONAL EXAMPLES
Passing \fB\-e\fR or \fB\-\-example\fR prints a complete working JSON authoring claim template and exits 0 immediately. It cannot be combined with \fICLAIMS_FILE\fR or \fB\-\-json\fR (exit status 2).
.SH ERRORS AND DIAGNOSTICS
If input cannot be parsed or violates ontology rules, KAT outputs explicit error information with line, column, and reason. In machine mode (\fB\-\-json\fR), error code \fBAUTHOR_PARSE_ERROR\fR (JSON syntax) or \fBAUTHOR_COMPILATION_FAILED\fR (domain validation failure) is returned in the result envelope. Endpoint constraint violations report provided source/target types alongside allowed source and target types.
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

    // 3. Generate kat-change(1) explicit transaction control man page
    let change_man = r#".TH kat\-change 1 "kat 0.4.2"
.SH NAME
kat\-change \- Manage draft Change transactions explicitly
.SH SYNOPSIS
.B kat change
<\fIsubcommand\fR>
.SH DESCRIPTION
Manage multi-operation change transactions explicitly.

For normal authoring, \fBkat-author\fR(1) automatically opens a draft Change when needed. Use \fBkat-change\fR(1) when explicit transaction control is required.
.SH SUBCOMMANDS
.TP
.B begin
Open a new multi-operation change transaction.
.TP
.B status
Inspect status and staged operations of the open change transaction.
.TP
.B commit
Commit all staged operations into a single ChangeRevision and publish.
.TP
.B abort
Abort open change transaction and discard all staged operations.
.SH SEE ALSO
.BR kat (1),
.BR kat\-author (1),
.BR kat\-commit (1),
.BR kat\-abort (1)
"#;
    fs::write(out_dir.join("kat-change.1"), change_man)?;

    // 4. Generate remaining subcommand man pages
    for sub in cmd.get_subcommands() {
        let sub_name = sub.get_name();
        if sub_name == "help" || sub_name == "author" || sub_name == "change" {
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
