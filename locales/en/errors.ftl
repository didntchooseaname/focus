errors-core-io = Could not access { $path }: { $detail }. Check that the file exists and that you can write to it.
errors-core-outside-vault = The path { $path } is outside the vault. Choose a location inside the vault.
errors-core-already-exists = { $path } already exists. Pick another name.
errors-core-unknown-note = This note no longer exists in the vault. Refresh the tree.
errors-core-conflict = { $path } changed on disk since it was last read. Reload it or keep your version.
errors-core-other = { $message }

errors-search-index-build = The search index could not be built: { $detail }
errors-search-index-write = The search index could not be saved: { $detail }

errors-ai-missing-key = No API key. Add your DeepSeek key in Settings, AI section.
errors-ai-invalid-key = DeepSeek refused the API key (401). Check it in Settings, AI section.
errors-ai-insufficient-balance = The DeepSeek balance is too low (402). Top up the account, then try again.
errors-ai-rate-limited = Rate limit reached (429). Try again in { $retry_after_secs } s.
errors-ai-bad-request = DeepSeek refused the request ({ $status }): { $message }
errors-ai-server = The DeepSeek server is unavailable ({ $status }). Try again in a moment.
errors-ai-network = Network error: { $detail }. Check your connection.
errors-ai-protocol = The response could not be read: { $detail }
errors-ai-cancelled = Cancelled

errors-ai-mcp-invalid-config = MCP server "{ $server }": invalid configuration: { $reason }
errors-ai-mcp-spawn = MCP server "{ $server }": could not start `{ $command }`: { $reason }. Check the command in Settings, AI section.
errors-ai-mcp-timeout = MCP server "{ $server }": no answer to `{ $method }` after { $secs } s. { $stderr }
errors-ai-mcp-exited = MCP server "{ $server }": the process stopped. { $detail }
errors-ai-mcp-protocol = MCP server "{ $server }": { $reason }
errors-ai-mcp-cancelled = Cancelled

errors-ops-io = Could not access { $path }: { $detail }. Check that the file exists and that you can write to it.
errors-ops-already-exists = The engagement { $path } already exists. Pick another name.
errors-ops-parse = { $file } could not be read: { $message }
errors-ops-invalid = { $message }
errors-ops-conflict = { $path } changed on disk since it was read. Reload it before saving.

errors-md-unsupported = This diagram type is not supported: { $kind }. The source is shown instead.
errors-md-syntax = Invalid Mermaid syntax: { $detail }
errors-md-render = The diagram could not be drawn: { $detail }

errors-mcp-not-running = The Focus application is not open on this vault. Open it, then try again.
errors-mcp-failed = { $message }

errors-vault-open = The vault could not be opened: { $detail }
errors-folder-picker = The folder picker failed: { $detail }
errors-theme = The theme could not be applied: { $detail }
errors-settings-read = The settings file could not be read: { $detail }. Defaults are used.
errors-settings-write = The settings could not be saved: { $detail }
errors-translation-file = Translation file ignored ({ $file }): { $detail }
errors-ai-keystore = The API key could not be stored in the system keyring: { $detail }
errors-copy-note = The note could not be copied: { $detail }
errors-export-report = The report could not be exported: { $detail }
clipboard-timeout = The clipboard did not answer within 3 seconds.
clipboard-too-large = The clipboard holds more than { $size }: nothing pasted.
clipboard-failed = { $tool } could not read the clipboard.
clipboard-start = { $tool } could not be started to read the clipboard.
