
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'kat' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'kat'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'kat' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show accepted repository state and current draft status')
            [CompletionResult]::new('context', 'context', [CompletionResultType]::ParameterValue, 'Retrieve bounded semantic development context around elements')
            [CompletionResult]::new('author', 'author', [CompletionResultType]::ParameterValue, 'Stage a semantic Change from declarative JSON')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Check consistency, evidence, accountability, and graph quality')
            [CompletionResult]::new('commit', 'commit', [CompletionResultType]::ParameterValue, 'Publish the current draft Change')
            [CompletionResult]::new('abort', 'abort', [CompletionResultType]::ParameterValue, 'Discard the current draft Change')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List knowledge elements in the current accepted state')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Inspect a resolved knowledge element')
            [CompletionResult]::new('history', 'history', [CompletionResultType]::ParameterValue, 'Show accepted Change history')
            [CompletionResult]::new('trace', 'trace', [CompletionResultType]::ParameterValue, 'Trace an element to its semantic origin')
            [CompletionResult]::new('impact', 'impact', [CompletionResultType]::ParameterValue, 'Analyze consequences of changing an element')
            [CompletionResult]::new('artifacts', 'artifacts', [CompletionResultType]::ParameterValue, 'Evaluate artifact accountability baselines')
            [CompletionResult]::new('ontology', 'ontology', [CompletionResultType]::ParameterValue, 'Discover semantic types and valid relationships')
            [CompletionResult]::new('validate', 'validate', [CompletionResultType]::ParameterValue, 'Run mechanical consistency validation')
            [CompletionResult]::new('create', 'create', [CompletionResultType]::ParameterValue, 'Create a knowledge element (prefer kat author for normal authoring)')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'Update an active knowledge element (prefer kat author for normal authoring)')
            [CompletionResult]::new('deprecate', 'deprecate', [CompletionResultType]::ParameterValue, 'Deprecate an active knowledge element (prefer kat author for normal authoring)')
            [CompletionResult]::new('supersede', 'supersede', [CompletionResultType]::ParameterValue, 'Replace an element while preserving semantic history (prefer kat author for normal authoring)')
            [CompletionResult]::new('link', 'link', [CompletionResultType]::ParameterValue, 'Establish a semantic relationship (prefer kat author for normal authoring)')
            [CompletionResult]::new('unlink', 'unlink', [CompletionResultType]::ParameterValue, 'Remove a semantic relationship (prefer kat author for normal authoring)')
            [CompletionResult]::new('account', 'account', [CompletionResultType]::ParameterValue, 'Re-baseline artifact accountability (prefer kat author for normal authoring)')
            [CompletionResult]::new('change', 'change', [CompletionResultType]::ParameterValue, 'Manage draft Change transactions explicitly')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Initialize a KAT repository')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'kat;status' {
            [CompletionResult]::new('--compact', '--compact', [CompletionResultType]::ParameterName, 'Display compact single-line dashboard')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'kat;context' {
            [CompletionResult]::new('--direction', '--direction', [CompletionResultType]::ParameterName, 'Traversal direction (upstream, downstream, both)')
            [CompletionResult]::new('--depth', '--depth', [CompletionResultType]::ParameterName, 'Maximum depth of relationship hops')
            [CompletionResult]::new('--categorize', '--categorize', [CompletionResultType]::ParameterName, 'Group context elements by ontology category')
            [CompletionResult]::new('--compact', '--compact', [CompletionResultType]::ParameterName, 'Display compact single-line per element layout')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'kat;author' {
            [CompletionResult]::new('-e', '-e', [CompletionResultType]::ParameterName, 'Print a complete working JSON example and exit')
            [CompletionResult]::new('--example', '--example', [CompletionResultType]::ParameterName, 'Print a complete working JSON example and exit')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'kat;check' {
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('--compact', '--compact', [CompletionResultType]::ParameterName, 'Display compact single-line summary')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'kat;commit' {
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'kat;abort' {
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'kat;list' {
            [CompletionResult]::new('--type', '--type', [CompletionResultType]::ParameterName, 'Filter by element type (e.g. requirement, design-decision)')
            [CompletionResult]::new('--lifecycle', '--lifecycle', [CompletionResultType]::ParameterName, 'Filter by lifecycle state (active, deprecated, superseded)')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;show' {
            [CompletionResult]::new('--compact', '--compact', [CompletionResultType]::ParameterName, 'Display compact single-line element summary')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;history' {
            [CompletionResult]::new('--limit', '--limit', [CompletionResultType]::ParameterName, 'Limit output to the N most recent revisions')
            [CompletionResult]::new('--element', '--element', [CompletionResultType]::ParameterName, 'Filter history to revisions touching a specific element ID or prefix')
            [CompletionResult]::new('--oneline', '--oneline', [CompletionResultType]::ParameterName, 'Format each history entry as a single line')
            [CompletionResult]::new('--compact', '--compact', [CompletionResultType]::ParameterName, 'Display compact output')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;trace' {
            [CompletionResult]::new('--max-depth', '--max-depth', [CompletionResultType]::ParameterName, 'Limit traversal depth to N relationship hops')
            [CompletionResult]::new('--paths', '--paths', [CompletionResultType]::ParameterName, 'Display explicit exhaustive path list instead of collapsed tree hierarchy')
            [CompletionResult]::new('--compact', '--compact', [CompletionResultType]::ParameterName, 'Display compact arrow-joined path rendering')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;impact' {
            [CompletionResult]::new('--max-depth', '--max-depth', [CompletionResultType]::ParameterName, 'Limit impact propagation depth to N relationship hops')
            [CompletionResult]::new('--compact', '--compact', [CompletionResultType]::ParameterName, 'Display compact flat table layout')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;artifacts' {
            [CompletionResult]::new('--stale', '--stale', [CompletionResultType]::ParameterName, 'Filter accountability report to display only STALE artifacts')
            [CompletionResult]::new('--compact', '--compact', [CompletionResultType]::ParameterName, 'Display compact status table layout')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;ontology' {
            [CompletionResult]::new('--compact', '--compact', [CompletionResultType]::ParameterName, 'Display compact shortened type IDs without human-readable names')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Inspect detailed capabilities and endpoint admissibility for a type')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'kat;ontology;show' {
            [CompletionResult]::new('--compact', '--compact', [CompletionResultType]::ParameterName, 'Display compact shortened type IDs without human-readable names')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;ontology;help' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Inspect detailed capabilities and endpoint admissibility for a type')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'kat;ontology;help;show' {
            break
        }
        'kat;ontology;help;help' {
            break
        }
        'kat;validate' {
            [CompletionResult]::new('--coverage', '--coverage', [CompletionResultType]::ParameterName, 'Focus on validation evidence coverage reporting across knowledge categories')
            [CompletionResult]::new('--compact', '--compact', [CompletionResultType]::ParameterName, 'Display compact single-line counts summary')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'kat;create' {
            [CompletionResult]::new('--title', '--title', [CompletionResultType]::ParameterName, 'Title of the knowledge element')
            [CompletionResult]::new('--description', '--description', [CompletionResultType]::ParameterName, 'Optional detailed description')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;update' {
            [CompletionResult]::new('--title', '--title', [CompletionResultType]::ParameterName, 'New title for the element')
            [CompletionResult]::new('--description', '--description', [CompletionResultType]::ParameterName, 'New description for the element')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;deprecate' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;supersede' {
            [CompletionResult]::new('--title', '--title', [CompletionResultType]::ParameterName, 'Title for the replacement element')
            [CompletionResult]::new('--description', '--description', [CompletionResultType]::ParameterName, 'Optional detailed description for the replacement element')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;link' {
            [CompletionResult]::new('--description', '--description', [CompletionResultType]::ParameterName, 'Optional detailed description')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
        'kat;unlink' {
            [CompletionResult]::new('--description', '--description', [CompletionResultType]::ParameterName, 'Optional detailed description')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;account' {
            [CompletionResult]::new('--description', '--description', [CompletionResultType]::ParameterName, 'Optional detailed description')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;change' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('begin', 'begin', [CompletionResultType]::ParameterValue, 'Open a new multi-operation change transaction')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Inspect status and staged operations of the open change transaction')
            [CompletionResult]::new('commit', 'commit', [CompletionResultType]::ParameterValue, 'Commit all staged operations into a single ChangeRevision and publish')
            [CompletionResult]::new('abort', 'abort', [CompletionResultType]::ParameterValue, 'Abort the open change transaction and discard all staged operations')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'kat;change;begin' {
            [CompletionResult]::new('--description', '--description', [CompletionResultType]::ParameterName, 'Optional change description')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;change;status' {
            [CompletionResult]::new('--compact', '--compact', [CompletionResultType]::ParameterName, 'Display compact summary')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;change;commit' {
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;change;abort' {
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output structured machine JSON envelope')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;change;help' {
            [CompletionResult]::new('begin', 'begin', [CompletionResultType]::ParameterValue, 'Open a new multi-operation change transaction')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Inspect status and staged operations of the open change transaction')
            [CompletionResult]::new('commit', 'commit', [CompletionResultType]::ParameterValue, 'Commit all staged operations into a single ChangeRevision and publish')
            [CompletionResult]::new('abort', 'abort', [CompletionResultType]::ParameterValue, 'Abort the open change transaction and discard all staged operations')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'kat;change;help;begin' {
            break
        }
        'kat;change;help;status' {
            break
        }
        'kat;change;help;commit' {
            break
        }
        'kat;change;help;abort' {
            break
        }
        'kat;change;help;help' {
            break
        }
        'kat;init' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'kat;help' {
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Show accepted repository state and current draft status')
            [CompletionResult]::new('context', 'context', [CompletionResultType]::ParameterValue, 'Retrieve bounded semantic development context around elements')
            [CompletionResult]::new('author', 'author', [CompletionResultType]::ParameterValue, 'Stage a semantic Change from declarative JSON')
            [CompletionResult]::new('check', 'check', [CompletionResultType]::ParameterValue, 'Check consistency, evidence, accountability, and graph quality')
            [CompletionResult]::new('commit', 'commit', [CompletionResultType]::ParameterValue, 'Publish the current draft Change')
            [CompletionResult]::new('abort', 'abort', [CompletionResultType]::ParameterValue, 'Discard the current draft Change')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List knowledge elements in the current accepted state')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Inspect a resolved knowledge element')
            [CompletionResult]::new('history', 'history', [CompletionResultType]::ParameterValue, 'Show accepted Change history')
            [CompletionResult]::new('trace', 'trace', [CompletionResultType]::ParameterValue, 'Trace an element to its semantic origin')
            [CompletionResult]::new('impact', 'impact', [CompletionResultType]::ParameterValue, 'Analyze consequences of changing an element')
            [CompletionResult]::new('artifacts', 'artifacts', [CompletionResultType]::ParameterValue, 'Evaluate artifact accountability baselines')
            [CompletionResult]::new('ontology', 'ontology', [CompletionResultType]::ParameterValue, 'Discover semantic types and valid relationships')
            [CompletionResult]::new('validate', 'validate', [CompletionResultType]::ParameterValue, 'Run mechanical consistency validation')
            [CompletionResult]::new('create', 'create', [CompletionResultType]::ParameterValue, 'Create a knowledge element (prefer kat author for normal authoring)')
            [CompletionResult]::new('update', 'update', [CompletionResultType]::ParameterValue, 'Update an active knowledge element (prefer kat author for normal authoring)')
            [CompletionResult]::new('deprecate', 'deprecate', [CompletionResultType]::ParameterValue, 'Deprecate an active knowledge element (prefer kat author for normal authoring)')
            [CompletionResult]::new('supersede', 'supersede', [CompletionResultType]::ParameterValue, 'Replace an element while preserving semantic history (prefer kat author for normal authoring)')
            [CompletionResult]::new('link', 'link', [CompletionResultType]::ParameterValue, 'Establish a semantic relationship (prefer kat author for normal authoring)')
            [CompletionResult]::new('unlink', 'unlink', [CompletionResultType]::ParameterValue, 'Remove a semantic relationship (prefer kat author for normal authoring)')
            [CompletionResult]::new('account', 'account', [CompletionResultType]::ParameterValue, 'Re-baseline artifact accountability (prefer kat author for normal authoring)')
            [CompletionResult]::new('change', 'change', [CompletionResultType]::ParameterValue, 'Manage draft Change transactions explicitly')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Initialize a KAT repository')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'kat;help;status' {
            break
        }
        'kat;help;context' {
            break
        }
        'kat;help;author' {
            break
        }
        'kat;help;check' {
            break
        }
        'kat;help;commit' {
            break
        }
        'kat;help;abort' {
            break
        }
        'kat;help;list' {
            break
        }
        'kat;help;show' {
            break
        }
        'kat;help;history' {
            break
        }
        'kat;help;trace' {
            break
        }
        'kat;help;impact' {
            break
        }
        'kat;help;artifacts' {
            break
        }
        'kat;help;ontology' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Inspect detailed capabilities and endpoint admissibility for a type')
            break
        }
        'kat;help;ontology;show' {
            break
        }
        'kat;help;validate' {
            break
        }
        'kat;help;create' {
            break
        }
        'kat;help;update' {
            break
        }
        'kat;help;deprecate' {
            break
        }
        'kat;help;supersede' {
            break
        }
        'kat;help;link' {
            break
        }
        'kat;help;unlink' {
            break
        }
        'kat;help;account' {
            break
        }
        'kat;help;change' {
            [CompletionResult]::new('begin', 'begin', [CompletionResultType]::ParameterValue, 'Open a new multi-operation change transaction')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Inspect status and staged operations of the open change transaction')
            [CompletionResult]::new('commit', 'commit', [CompletionResultType]::ParameterValue, 'Commit all staged operations into a single ChangeRevision and publish')
            [CompletionResult]::new('abort', 'abort', [CompletionResultType]::ParameterValue, 'Abort the open change transaction and discard all staged operations')
            break
        }
        'kat;help;change;begin' {
            break
        }
        'kat;help;change;status' {
            break
        }
        'kat;help;change;commit' {
            break
        }
        'kat;help;change;abort' {
            break
        }
        'kat;help;init' {
            break
        }
        'kat;help;help' {
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
