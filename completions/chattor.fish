# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_chattor_global_optspecs
	string join \n d/debug c/config-dir= data-dir= h/help
end

function __fish_chattor_needs_command
	# Figure out if the current invocation already has a command.
	set -l cmd (commandline -opc)
	set -e cmd[1]
	argparse -s (__fish_chattor_global_optspecs) -- $cmd 2>/dev/null
	or return
	if set -q argv[1]
		# Also print the command, so this can be used to figure out what it is.
		echo $argv[1]
		return 1
	end
	return 0
end

function __fish_chattor_using_subcommand
	set -l cmd (__fish_chattor_needs_command)
	test -z "$cmd"
	and return 1
	contains -- $cmd[1] $argv
end

complete -c chattor -n "__fish_chattor_needs_command" -s c -l config-dir -d 'Override config directory' -r
complete -c chattor -n "__fish_chattor_needs_command" -l data-dir -d 'Override data directory' -r
complete -c chattor -n "__fish_chattor_needs_command" -s d -l debug -d 'Enable debug logging'
complete -c chattor -n "__fish_chattor_needs_command" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_needs_command" -f -a "tui" -d 'Run interactive TUI (default if no subcommand)'
complete -c chattor -n "__fish_chattor_needs_command" -f -a "daemon" -d 'Run headless daemon'
complete -c chattor -n "__fish_chattor_needs_command" -f -a "status" -d 'Show daemon status'
complete -c chattor -n "__fish_chattor_needs_command" -f -a "identity" -d 'Show own identity'
complete -c chattor -n "__fish_chattor_needs_command" -f -a "friends" -d 'Friend management'
complete -c chattor -n "__fish_chattor_needs_command" -f -a "send" -d 'Send a message'
complete -c chattor -n "__fish_chattor_needs_command" -f -a "recv" -d 'Receive unread messages'
complete -c chattor -n "__fish_chattor_needs_command" -f -a "listen" -d 'Stream incoming messages (blocking)'
complete -c chattor -n "__fish_chattor_needs_command" -f -a "channels" -d 'Channel management'
complete -c chattor -n "__fish_chattor_needs_command" -f -a "ephemeral" -d 'Set ephemeral message TTL'
complete -c chattor -n "__fish_chattor_needs_command" -f -a "notifications" -d 'Toggle notifications'
complete -c chattor -n "__fish_chattor_needs_command" -f -a "mcp" -d 'Start MCP server (stdio transport)'
complete -c chattor -n "__fish_chattor_needs_command" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c chattor -n "__fish_chattor_using_subcommand tui" -s t -l theme -d 'Theme preset (dark, light, cyberpunk, minimal, rose-pine, rose-pine-moon, rose-pine-dawn)' -r
complete -c chattor -n "__fish_chattor_using_subcommand tui" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand daemon" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand status" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand identity" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and not __fish_seen_subcommand_from list add remove requests accept reject help" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and not __fish_seen_subcommand_from list add remove requests accept reject help" -f -a "list" -d 'List friends'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and not __fish_seen_subcommand_from list add remove requests accept reject help" -f -a "add" -d 'Add a friend'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and not __fish_seen_subcommand_from list add remove requests accept reject help" -f -a "remove" -d 'Remove a friend'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and not __fish_seen_subcommand_from list add remove requests accept reject help" -f -a "requests" -d 'List pending requests'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and not __fish_seen_subcommand_from list add remove requests accept reject help" -f -a "accept" -d 'Accept a friend request'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and not __fish_seen_subcommand_from list add remove requests accept reject help" -f -a "reject" -d 'Reject a friend request'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and not __fish_seen_subcommand_from list add remove requests accept reject help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and __fish_seen_subcommand_from requests" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and __fish_seen_subcommand_from accept" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and __fish_seen_subcommand_from reject" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and __fish_seen_subcommand_from help" -f -a "list" -d 'List friends'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and __fish_seen_subcommand_from help" -f -a "add" -d 'Add a friend'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Remove a friend'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and __fish_seen_subcommand_from help" -f -a "requests" -d 'List pending requests'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and __fish_seen_subcommand_from help" -f -a "accept" -d 'Accept a friend request'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and __fish_seen_subcommand_from help" -f -a "reject" -d 'Reject a friend request'
complete -c chattor -n "__fish_chattor_using_subcommand friends; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c chattor -n "__fish_chattor_using_subcommand send" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand recv" -l peer -r
complete -c chattor -n "__fish_chattor_using_subcommand recv" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand listen" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and not __fish_seen_subcommand_from list publish subscribe feed help" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and not __fish_seen_subcommand_from list publish subscribe feed help" -f -a "list" -d 'List channels'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and not __fish_seen_subcommand_from list publish subscribe feed help" -f -a "publish" -d 'Publish to a channel'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and not __fish_seen_subcommand_from list publish subscribe feed help" -f -a "subscribe" -d 'Subscribe to a channel'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and not __fish_seen_subcommand_from list publish subscribe feed help" -f -a "feed" -d 'Read channel feed'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and not __fish_seen_subcommand_from list publish subscribe feed help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and __fish_seen_subcommand_from publish" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and __fish_seen_subcommand_from subscribe" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and __fish_seen_subcommand_from feed" -l channel -r
complete -c chattor -n "__fish_chattor_using_subcommand channels; and __fish_seen_subcommand_from feed" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and __fish_seen_subcommand_from help" -f -a "list" -d 'List channels'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and __fish_seen_subcommand_from help" -f -a "publish" -d 'Publish to a channel'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and __fish_seen_subcommand_from help" -f -a "subscribe" -d 'Subscribe to a channel'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and __fish_seen_subcommand_from help" -f -a "feed" -d 'Read channel feed'
complete -c chattor -n "__fish_chattor_using_subcommand channels; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c chattor -n "__fish_chattor_using_subcommand ephemeral" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand notifications" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand mcp" -s h -l help -d 'Print help'
complete -c chattor -n "__fish_chattor_using_subcommand help; and not __fish_seen_subcommand_from tui daemon status identity friends send recv listen channels ephemeral notifications mcp help" -f -a "tui" -d 'Run interactive TUI (default if no subcommand)'
complete -c chattor -n "__fish_chattor_using_subcommand help; and not __fish_seen_subcommand_from tui daemon status identity friends send recv listen channels ephemeral notifications mcp help" -f -a "daemon" -d 'Run headless daemon'
complete -c chattor -n "__fish_chattor_using_subcommand help; and not __fish_seen_subcommand_from tui daemon status identity friends send recv listen channels ephemeral notifications mcp help" -f -a "status" -d 'Show daemon status'
complete -c chattor -n "__fish_chattor_using_subcommand help; and not __fish_seen_subcommand_from tui daemon status identity friends send recv listen channels ephemeral notifications mcp help" -f -a "identity" -d 'Show own identity'
complete -c chattor -n "__fish_chattor_using_subcommand help; and not __fish_seen_subcommand_from tui daemon status identity friends send recv listen channels ephemeral notifications mcp help" -f -a "friends" -d 'Friend management'
complete -c chattor -n "__fish_chattor_using_subcommand help; and not __fish_seen_subcommand_from tui daemon status identity friends send recv listen channels ephemeral notifications mcp help" -f -a "send" -d 'Send a message'
complete -c chattor -n "__fish_chattor_using_subcommand help; and not __fish_seen_subcommand_from tui daemon status identity friends send recv listen channels ephemeral notifications mcp help" -f -a "recv" -d 'Receive unread messages'
complete -c chattor -n "__fish_chattor_using_subcommand help; and not __fish_seen_subcommand_from tui daemon status identity friends send recv listen channels ephemeral notifications mcp help" -f -a "listen" -d 'Stream incoming messages (blocking)'
complete -c chattor -n "__fish_chattor_using_subcommand help; and not __fish_seen_subcommand_from tui daemon status identity friends send recv listen channels ephemeral notifications mcp help" -f -a "channels" -d 'Channel management'
complete -c chattor -n "__fish_chattor_using_subcommand help; and not __fish_seen_subcommand_from tui daemon status identity friends send recv listen channels ephemeral notifications mcp help" -f -a "ephemeral" -d 'Set ephemeral message TTL'
complete -c chattor -n "__fish_chattor_using_subcommand help; and not __fish_seen_subcommand_from tui daemon status identity friends send recv listen channels ephemeral notifications mcp help" -f -a "notifications" -d 'Toggle notifications'
complete -c chattor -n "__fish_chattor_using_subcommand help; and not __fish_seen_subcommand_from tui daemon status identity friends send recv listen channels ephemeral notifications mcp help" -f -a "mcp" -d 'Start MCP server (stdio transport)'
complete -c chattor -n "__fish_chattor_using_subcommand help; and not __fish_seen_subcommand_from tui daemon status identity friends send recv listen channels ephemeral notifications mcp help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c chattor -n "__fish_chattor_using_subcommand help; and __fish_seen_subcommand_from friends" -f -a "list" -d 'List friends'
complete -c chattor -n "__fish_chattor_using_subcommand help; and __fish_seen_subcommand_from friends" -f -a "add" -d 'Add a friend'
complete -c chattor -n "__fish_chattor_using_subcommand help; and __fish_seen_subcommand_from friends" -f -a "remove" -d 'Remove a friend'
complete -c chattor -n "__fish_chattor_using_subcommand help; and __fish_seen_subcommand_from friends" -f -a "requests" -d 'List pending requests'
complete -c chattor -n "__fish_chattor_using_subcommand help; and __fish_seen_subcommand_from friends" -f -a "accept" -d 'Accept a friend request'
complete -c chattor -n "__fish_chattor_using_subcommand help; and __fish_seen_subcommand_from friends" -f -a "reject" -d 'Reject a friend request'
complete -c chattor -n "__fish_chattor_using_subcommand help; and __fish_seen_subcommand_from channels" -f -a "list" -d 'List channels'
complete -c chattor -n "__fish_chattor_using_subcommand help; and __fish_seen_subcommand_from channels" -f -a "publish" -d 'Publish to a channel'
complete -c chattor -n "__fish_chattor_using_subcommand help; and __fish_seen_subcommand_from channels" -f -a "subscribe" -d 'Subscribe to a channel'
complete -c chattor -n "__fish_chattor_using_subcommand help; and __fish_seen_subcommand_from channels" -f -a "feed" -d 'Read channel feed'
