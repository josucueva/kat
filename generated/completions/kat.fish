# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_kat_global_optspecs
    string join \n h/help V/version
end

function __fish_kat_needs_command
    # Figure out if the current invocation already has a command.
    set -l cmd (commandline -opc)
    set -e cmd[1]
    argparse -s (__fish_kat_global_optspecs) -- $cmd 2>/dev/null
    or return
    if set -q argv[1]
        # Also print the command, so this can be used to figure out what it is.
        echo $argv[1]
        return 1
    end
    return 0
end

function __fish_kat_using_subcommand
    set -l cmd (__fish_kat_needs_command)
    test -z "$cmd"
    and return 1
    contains -- $cmd[1] $argv
end

complete -c kat -n "__fish_kat_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c kat -n "__fish_kat_needs_command" -s V -l version -d 'Print version'
complete -c kat -n "__fish_kat_needs_command" -f -a "status" -d 'Show accepted repository state and current draft status'
complete -c kat -n "__fish_kat_needs_command" -f -a "context" -d 'Retrieve bounded semantic development context around elements'
complete -c kat -n "__fish_kat_needs_command" -f -a "author" -d 'Stage a semantic Change from declarative JSON'
complete -c kat -n "__fish_kat_needs_command" -f -a "check" -d 'Check consistency, evidence, accountability, and graph quality'
complete -c kat -n "__fish_kat_needs_command" -f -a "commit" -d 'Publish the current draft Change'
complete -c kat -n "__fish_kat_needs_command" -f -a "abort" -d 'Discard the current draft Change'
complete -c kat -n "__fish_kat_needs_command" -f -a "list" -d 'List knowledge elements in the current accepted state'
complete -c kat -n "__fish_kat_needs_command" -f -a "show" -d 'Inspect a resolved knowledge element'
complete -c kat -n "__fish_kat_needs_command" -f -a "history" -d 'Show accepted Change history'
complete -c kat -n "__fish_kat_needs_command" -f -a "trace" -d 'Trace an element to its semantic origin'
complete -c kat -n "__fish_kat_needs_command" -f -a "impact" -d 'Analyze consequences of changing an element'
complete -c kat -n "__fish_kat_needs_command" -f -a "artifacts" -d 'Evaluate artifact accountability baselines'
complete -c kat -n "__fish_kat_needs_command" -f -a "ontology" -d 'Discover semantic types and valid relationships'
complete -c kat -n "__fish_kat_needs_command" -f -a "validate" -d 'Run mechanical consistency validation'
complete -c kat -n "__fish_kat_needs_command" -f -a "create" -d 'Create a knowledge element (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_needs_command" -f -a "update" -d 'Update an active knowledge element (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_needs_command" -f -a "deprecate" -d 'Deprecate an active knowledge element (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_needs_command" -f -a "supersede" -d 'Replace an element while preserving semantic history (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_needs_command" -f -a "link" -d 'Establish a semantic relationship (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_needs_command" -f -a "unlink" -d 'Remove a semantic relationship (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_needs_command" -f -a "account" -d 'Re-baseline artifact accountability (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_needs_command" -f -a "change" -d 'Manage draft Change transactions explicitly'
complete -c kat -n "__fish_kat_needs_command" -f -a "init" -d 'Initialize a KAT repository'
complete -c kat -n "__fish_kat_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c kat -n "__fish_kat_using_subcommand status" -l compact -d 'Display compact single-line dashboard'
complete -c kat -n "__fish_kat_using_subcommand status" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand status" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c kat -n "__fish_kat_using_subcommand context" -l direction -d 'Traversal direction (upstream, downstream, both)' -r
complete -c kat -n "__fish_kat_using_subcommand context" -l depth -d 'Maximum depth of relationship hops' -r
complete -c kat -n "__fish_kat_using_subcommand context" -l categorize -d 'Group context elements by ontology category'
complete -c kat -n "__fish_kat_using_subcommand context" -l compact -d 'Display compact single-line per element layout'
complete -c kat -n "__fish_kat_using_subcommand context" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand context" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c kat -n "__fish_kat_using_subcommand author" -s e -l example -d 'Print a complete working JSON example and exit'
complete -c kat -n "__fish_kat_using_subcommand author" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand author" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c kat -n "__fish_kat_using_subcommand check" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand check" -l compact -d 'Display compact single-line summary'
complete -c kat -n "__fish_kat_using_subcommand check" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c kat -n "__fish_kat_using_subcommand commit" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand commit" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c kat -n "__fish_kat_using_subcommand abort" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand abort" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c kat -n "__fish_kat_using_subcommand list" -l type -d 'Filter by element type (e.g. requirement, design-decision)' -r
complete -c kat -n "__fish_kat_using_subcommand list" -l lifecycle -d 'Filter by lifecycle state (active, deprecated, superseded)' -r
complete -c kat -n "__fish_kat_using_subcommand list" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand list" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand show" -l compact -d 'Display compact single-line element summary'
complete -c kat -n "__fish_kat_using_subcommand show" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand show" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand history" -l limit -d 'Limit output to the N most recent revisions' -r
complete -c kat -n "__fish_kat_using_subcommand history" -l element -d 'Filter history to revisions touching a specific element ID or prefix' -r
complete -c kat -n "__fish_kat_using_subcommand history" -l oneline -d 'Format each history entry as a single line'
complete -c kat -n "__fish_kat_using_subcommand history" -l compact -d 'Display compact output'
complete -c kat -n "__fish_kat_using_subcommand history" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand history" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand trace" -l max-depth -d 'Limit traversal depth to N relationship hops' -r
complete -c kat -n "__fish_kat_using_subcommand trace" -l paths -d 'Display explicit exhaustive path list instead of collapsed tree hierarchy'
complete -c kat -n "__fish_kat_using_subcommand trace" -l compact -d 'Display compact arrow-joined path rendering'
complete -c kat -n "__fish_kat_using_subcommand trace" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand trace" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand impact" -l max-depth -d 'Limit impact propagation depth to N relationship hops' -r
complete -c kat -n "__fish_kat_using_subcommand impact" -l compact -d 'Display compact flat table layout'
complete -c kat -n "__fish_kat_using_subcommand impact" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand impact" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand artifacts" -l stale -d 'Filter accountability report to display only STALE artifacts'
complete -c kat -n "__fish_kat_using_subcommand artifacts" -l compact -d 'Display compact status table layout'
complete -c kat -n "__fish_kat_using_subcommand artifacts" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand artifacts" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand ontology; and not __fish_seen_subcommand_from show help" -l compact -d 'Display compact shortened type IDs without human-readable names'
complete -c kat -n "__fish_kat_using_subcommand ontology; and not __fish_seen_subcommand_from show help" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand ontology; and not __fish_seen_subcommand_from show help" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand ontology; and not __fish_seen_subcommand_from show help" -f -a "show" -d 'Inspect detailed capabilities and endpoint admissibility for a type'
complete -c kat -n "__fish_kat_using_subcommand ontology; and not __fish_seen_subcommand_from show help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c kat -n "__fish_kat_using_subcommand ontology; and __fish_seen_subcommand_from show" -l compact -d 'Display compact shortened type IDs without human-readable names'
complete -c kat -n "__fish_kat_using_subcommand ontology; and __fish_seen_subcommand_from show" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand ontology; and __fish_seen_subcommand_from show" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand ontology; and __fish_seen_subcommand_from help" -f -a "show" -d 'Inspect detailed capabilities and endpoint admissibility for a type'
complete -c kat -n "__fish_kat_using_subcommand ontology; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c kat -n "__fish_kat_using_subcommand validate" -l coverage -d 'Focus on validation evidence coverage reporting across knowledge categories'
complete -c kat -n "__fish_kat_using_subcommand validate" -l compact -d 'Display compact single-line counts summary'
complete -c kat -n "__fish_kat_using_subcommand validate" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand validate" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c kat -n "__fish_kat_using_subcommand create" -l title -d 'Title of the knowledge element' -r
complete -c kat -n "__fish_kat_using_subcommand create" -l description -d 'Optional detailed description' -r
complete -c kat -n "__fish_kat_using_subcommand create" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand update" -l title -d 'New title for the element' -r
complete -c kat -n "__fish_kat_using_subcommand update" -l description -d 'New description for the element' -r
complete -c kat -n "__fish_kat_using_subcommand update" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand deprecate" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand supersede" -l title -d 'Title for the replacement element' -r
complete -c kat -n "__fish_kat_using_subcommand supersede" -l description -d 'Optional detailed description for the replacement element' -r
complete -c kat -n "__fish_kat_using_subcommand supersede" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand link" -l description -d 'Optional detailed description' -r
complete -c kat -n "__fish_kat_using_subcommand link" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c kat -n "__fish_kat_using_subcommand unlink" -l description -d 'Optional detailed description' -r
complete -c kat -n "__fish_kat_using_subcommand unlink" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand account" -l description -d 'Optional detailed description' -r
complete -c kat -n "__fish_kat_using_subcommand account" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand change; and not __fish_seen_subcommand_from begin status commit abort help" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c kat -n "__fish_kat_using_subcommand change; and not __fish_seen_subcommand_from begin status commit abort help" -f -a "begin" -d 'Open a new multi-operation change transaction'
complete -c kat -n "__fish_kat_using_subcommand change; and not __fish_seen_subcommand_from begin status commit abort help" -f -a "status" -d 'Inspect status and staged operations of the open change transaction'
complete -c kat -n "__fish_kat_using_subcommand change; and not __fish_seen_subcommand_from begin status commit abort help" -f -a "commit" -d 'Commit all staged operations into a single ChangeRevision and publish'
complete -c kat -n "__fish_kat_using_subcommand change; and not __fish_seen_subcommand_from begin status commit abort help" -f -a "abort" -d 'Abort the open change transaction and discard all staged operations'
complete -c kat -n "__fish_kat_using_subcommand change; and not __fish_seen_subcommand_from begin status commit abort help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from begin" -l description -d 'Optional change description' -r
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from begin" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from begin" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from status" -l compact -d 'Display compact summary'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from status" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from commit" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from commit" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from abort" -l json -d 'Output structured machine JSON envelope'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from abort" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from help" -f -a "begin" -d 'Open a new multi-operation change transaction'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from help" -f -a "status" -d 'Inspect status and staged operations of the open change transaction'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from help" -f -a "commit" -d 'Commit all staged operations into a single ChangeRevision and publish'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from help" -f -a "abort" -d 'Abort the open change transaction and discard all staged operations'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c kat -n "__fish_kat_using_subcommand init" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "status" -d 'Show accepted repository state and current draft status'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "context" -d 'Retrieve bounded semantic development context around elements'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "author" -d 'Stage a semantic Change from declarative JSON'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "check" -d 'Check consistency, evidence, accountability, and graph quality'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "commit" -d 'Publish the current draft Change'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "abort" -d 'Discard the current draft Change'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "list" -d 'List knowledge elements in the current accepted state'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "show" -d 'Inspect a resolved knowledge element'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "history" -d 'Show accepted Change history'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "trace" -d 'Trace an element to its semantic origin'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "impact" -d 'Analyze consequences of changing an element'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "artifacts" -d 'Evaluate artifact accountability baselines'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "ontology" -d 'Discover semantic types and valid relationships'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "validate" -d 'Run mechanical consistency validation'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "create" -d 'Create a knowledge element (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "update" -d 'Update an active knowledge element (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "deprecate" -d 'Deprecate an active knowledge element (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "supersede" -d 'Replace an element while preserving semantic history (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "link" -d 'Establish a semantic relationship (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "unlink" -d 'Remove a semantic relationship (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "account" -d 'Re-baseline artifact accountability (prefer kat author for normal authoring)'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "change" -d 'Manage draft Change transactions explicitly'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "init" -d 'Initialize a KAT repository'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from status context author check commit abort list show history trace impact artifacts ontology validate create update deprecate supersede link unlink account change init help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c kat -n "__fish_kat_using_subcommand help; and __fish_seen_subcommand_from ontology" -f -a "show" -d 'Inspect detailed capabilities and endpoint admissibility for a type'
complete -c kat -n "__fish_kat_using_subcommand help; and __fish_seen_subcommand_from change" -f -a "begin" -d 'Open a new multi-operation change transaction'
complete -c kat -n "__fish_kat_using_subcommand help; and __fish_seen_subcommand_from change" -f -a "status" -d 'Inspect status and staged operations of the open change transaction'
complete -c kat -n "__fish_kat_using_subcommand help; and __fish_seen_subcommand_from change" -f -a "commit" -d 'Commit all staged operations into a single ChangeRevision and publish'
complete -c kat -n "__fish_kat_using_subcommand help; and __fish_seen_subcommand_from change" -f -a "abort" -d 'Abort the open change transaction and discard all staged operations'
