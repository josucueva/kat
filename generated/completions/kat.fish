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
complete -c kat -n "__fish_kat_needs_command" -f -a "init" -d 'Initialize a new KAT repository in the current directory'
complete -c kat -n "__fish_kat_needs_command" -f -a "status" -d 'Display a concise summary of current accepted repository status and health'
complete -c kat -n "__fish_kat_needs_command" -f -a "list" -d 'List knowledge elements in the current accepted state'
complete -c kat -n "__fish_kat_needs_command" -f -a "create" -d 'Create a new knowledge element'
complete -c kat -n "__fish_kat_needs_command" -f -a "update" -d 'Update title or description of an existing active knowledge element'
complete -c kat -n "__fish_kat_needs_command" -f -a "deprecate" -d 'Mark an active knowledge element as Deprecated'
complete -c kat -n "__fish_kat_needs_command" -f -a "supersede" -d 'Supersede an existing knowledge element with a new replacement element'
complete -c kat -n "__fish_kat_needs_command" -f -a "link" -d 'Establish a semantic relationship between two elements'
complete -c kat -n "__fish_kat_needs_command" -f -a "unlink" -d 'Remove a relationship from the current accepted state'
complete -c kat -n "__fish_kat_needs_command" -f -a "show" -d 'Show detailed view of a resolved active knowledge element'
complete -c kat -n "__fish_kat_needs_command" -f -a "history" -d 'Reconstruct and display the accepted change revision graph'
complete -c kat -n "__fish_kat_needs_command" -f -a "trace" -d 'Trace a knowledge element back to its origin'
complete -c kat -n "__fish_kat_needs_command" -f -a "impact" -d 'Analyze potential impact and consequences of changing an element'
complete -c kat -n "__fish_kat_needs_command" -f -a "validate" -d 'Run mechanical consistency validation across the current accepted state'
complete -c kat -n "__fish_kat_needs_command" -f -a "artifacts" -d 'Evaluate artifact accountability baselines against accepted state'
complete -c kat -n "__fish_kat_needs_command" -f -a "change" -d 'Manage multi-operation change transactions'
complete -c kat -n "__fish_kat_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c kat -n "__fish_kat_using_subcommand init" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand status" -l compact -d 'Display compact single-line dashboard'
complete -c kat -n "__fish_kat_using_subcommand status" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand list" -l type -d 'Filter by element type (e.g. requirement, design-decision)' -r
complete -c kat -n "__fish_kat_using_subcommand list" -l lifecycle -d 'Filter by lifecycle state (active, deprecated, superseded)' -r
complete -c kat -n "__fish_kat_using_subcommand list" -s h -l help -d 'Print help'
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
complete -c kat -n "__fish_kat_using_subcommand link" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand unlink" -l description -d 'Optional detailed description' -r
complete -c kat -n "__fish_kat_using_subcommand unlink" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand show" -l compact -d 'Display compact single-line element summary'
complete -c kat -n "__fish_kat_using_subcommand show" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand history" -l limit -d 'Limit output to the N most recent revisions' -r
complete -c kat -n "__fish_kat_using_subcommand history" -l element -d 'Filter history to revisions touching a specific element ID or prefix' -r
complete -c kat -n "__fish_kat_using_subcommand history" -l oneline -d 'Format each history entry as a single line'
complete -c kat -n "__fish_kat_using_subcommand history" -l compact -d 'Display compact output'
complete -c kat -n "__fish_kat_using_subcommand history" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand trace" -l compact -d 'Display compact arrow-joined path rendering'
complete -c kat -n "__fish_kat_using_subcommand trace" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand impact" -l compact -d 'Display compact flat table layout'
complete -c kat -n "__fish_kat_using_subcommand impact" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand validate" -l compact -d 'Display compact single-line counts summary'
complete -c kat -n "__fish_kat_using_subcommand validate" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand artifacts" -l compact -d 'Display compact status table layout'
complete -c kat -n "__fish_kat_using_subcommand artifacts" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand change; and not __fish_seen_subcommand_from begin status commit abort help" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand change; and not __fish_seen_subcommand_from begin status commit abort help" -f -a "begin" -d 'Open a new multi-operation change transaction'
complete -c kat -n "__fish_kat_using_subcommand change; and not __fish_seen_subcommand_from begin status commit abort help" -f -a "status" -d 'Inspect status and staged operations of the open change transaction'
complete -c kat -n "__fish_kat_using_subcommand change; and not __fish_seen_subcommand_from begin status commit abort help" -f -a "commit" -d 'Commit all staged operations into a single ChangeRevision and publish'
complete -c kat -n "__fish_kat_using_subcommand change; and not __fish_seen_subcommand_from begin status commit abort help" -f -a "abort" -d 'Abort the open change transaction and discard all staged operations'
complete -c kat -n "__fish_kat_using_subcommand change; and not __fish_seen_subcommand_from begin status commit abort help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from begin" -l description -d 'Optional change description' -r
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from begin" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from status" -l compact -d 'Display compact summary'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from status" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from commit" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from abort" -s h -l help -d 'Print help'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from help" -f -a "begin" -d 'Open a new multi-operation change transaction'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from help" -f -a "status" -d 'Inspect status and staged operations of the open change transaction'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from help" -f -a "commit" -d 'Commit all staged operations into a single ChangeRevision and publish'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from help" -f -a "abort" -d 'Abort the open change transaction and discard all staged operations'
complete -c kat -n "__fish_kat_using_subcommand change; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "init" -d 'Initialize a new KAT repository in the current directory'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "status" -d 'Display a concise summary of current accepted repository status and health'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "list" -d 'List knowledge elements in the current accepted state'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "create" -d 'Create a new knowledge element'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "update" -d 'Update title or description of an existing active knowledge element'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "deprecate" -d 'Mark an active knowledge element as Deprecated'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "supersede" -d 'Supersede an existing knowledge element with a new replacement element'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "link" -d 'Establish a semantic relationship between two elements'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "unlink" -d 'Remove a relationship from the current accepted state'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "show" -d 'Show detailed view of a resolved active knowledge element'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "history" -d 'Reconstruct and display the accepted change revision graph'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "trace" -d 'Trace a knowledge element back to its origin'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "impact" -d 'Analyze potential impact and consequences of changing an element'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "validate" -d 'Run mechanical consistency validation across the current accepted state'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "artifacts" -d 'Evaluate artifact accountability baselines against accepted state'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "change" -d 'Manage multi-operation change transactions'
complete -c kat -n "__fish_kat_using_subcommand help; and not __fish_seen_subcommand_from init status list create update deprecate supersede link unlink show history trace impact validate artifacts change help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c kat -n "__fish_kat_using_subcommand help; and __fish_seen_subcommand_from change" -f -a "begin" -d 'Open a new multi-operation change transaction'
complete -c kat -n "__fish_kat_using_subcommand help; and __fish_seen_subcommand_from change" -f -a "status" -d 'Inspect status and staged operations of the open change transaction'
complete -c kat -n "__fish_kat_using_subcommand help; and __fish_seen_subcommand_from change" -f -a "commit" -d 'Commit all staged operations into a single ChangeRevision and publish'
complete -c kat -n "__fish_kat_using_subcommand help; and __fish_seen_subcommand_from change" -f -a "abort" -d 'Abort the open change transaction and discard all staged operations'
