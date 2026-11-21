ai-title = AI
ai-input-placeholder = Ask about your notes
ai-send = Send
ai-stop = Stop
ai-retry = Try again
ai-copy = Copy
ai-insert-at-cursor = Insert at cursor
ai-save-as-note = Save as note
ai-new-conversation = New conversation
ai-history = History
ai-history-empty = No conversations yet.
ai-reasoning = Reasoning
ai-reasoning-show = Show reasoning
ai-reasoning-hide = Hide reasoning
ai-generating = Generating…
ai-tool-call = Tool call: { $name }
ai-tool-search-notes = Searching notes
ai-tool-read-note = Reading a note
ai-tool-list-folder = Listing a folder
ai-tool-get-backlinks = Reading backlinks
ai-tool-get-outline = Reading an outline
ai-tool-list-tags = Listing tags
ai-confirm-title = Confirm this change
ai-confirm-apply = Apply
ai-confirm-reject = Reject
ai-confirm-always-allow = Always allow this tool for { $server }
ai-diff-title = Differences
ai-context-note = Current note: { $path }
ai-context-selection = Selection
ai-context-added = Added with @: { $path }
ai-context-remove = Remove from context
ai-tokens-approx = ~{ $count }
ai-tokens-approx-thousands = ~{ $count }k
ai-context-tokens =
    { $count ->
        [one] About { $count } token
       *[other] About { $count } tokens
    }
ai-usage-tokens =
    { $count ->
        [one] { $count } token
       *[other] { $count } tokens
    }
ai-usage-cached = { $percent } % served from cache
ai-usage-cost = Estimated cost { $cost }
ai-balance = Balance: { $amount }
ai-redaction-notice =
    { $count ->
        [one] { $count } secret masked before sending
       *[other] { $count } secrets masked before sending
    }
ai-connection-ok = Connection works. Balance: { $amount }.
ai-connection-testing = Testing the connection…
ai-mcp-state-running = Running
ai-mcp-state-stopped = Stopped
ai-mcp-state-error = Error
ai-mcp-tools =
    { $count ->
        [one] { $count } tool
       *[other] { $count } tools
    }
ai-actions-edit = Edit actions
ai-action-duplicate = Duplicate
ai-action-hide = Hide
ai-action-restore = Restore
ai-action-builtin = Built in
ai-action-ops-only = Ops mode only

ai-action-summarize-label = Summarize
ai-action-summarize-desc = Central idea and key points of the selection.
ai-action-refine-label = Refine
ai-action-refine-desc = Improves clarity and structure without changing the content.
ai-action-rephrase-label = Rephrase
ai-action-rephrase-desc = Says the same thing with other words.
ai-action-correct-label = Correct
ai-action-correct-desc = Fixes spelling, grammar and punctuation.
ai-action-translate-fr-label = Translate to French
ai-action-translate-fr-desc = Keeps Markdown, code and links as they are.
ai-action-translate-en-label = Translate to English
ai-action-translate-en-desc = Keeps Markdown, code and links as they are.
ai-action-continue-label = Continue
ai-action-continue-desc = Writes the next paragraph in the same tone.
ai-action-explain-label = Explain
ai-action-explain-desc = Explains the text to a reader who knows the field.
ai-action-tasks-label = Extract tasks
ai-action-tasks-desc = Turns the text into a Markdown task list.
ai-action-title-tags-label = Suggest title and tags
ai-action-title-tags-desc = A short title and three to six tags for the note.
ai-action-mermaid-label = Generate a Mermaid diagram
ai-action-mermaid-desc = Draws the structure of the text as a diagram.
ai-action-note-from-outline-label = Generate a note from an outline
ai-action-note-from-outline-desc = Writes a full note from the selected outline.
ai-action-ops-finding-label = Write a finding
ai-action-ops-finding-desc = Description, impact, evidence, remediation and references from your notes.
ai-action-ops-exec-summary-label = Executive summary
ai-action-ops-exec-summary-desc = Summary of the engagement for a non-technical reader.
ai-action-ops-next-enum-label = Next enumeration steps
ai-action-ops-next-enum-desc = Next steps ranked from the most to the least worthwhile.

ai-output-chat = Show in the thread
ai-output-replace = Replace the text
ai-output-insert = Insert below
ai-output-note = Create a note

ai-model = Model
ai-actions = Actions
ai-add-context = Add a note to the context
ai-empty-title = Ask about your notes
ai-empty-hint = Add notes to the context with @, or run an action on the current note.
ai-no-key-title = No API key
ai-no-key-body = Add your { $provider } API key in Settings to ask questions and run actions.
ai-no-key-open = Open settings
ai-history-untitled = Untitled conversation
ai-history-delete = Delete conversation
ai-tool-refused = Refused
ai-tool-failed = Failed
ai-tool-no-output = The tool returned nothing.
ai-proposal-replace = Proposed replacement
ai-confirm-applied = Applied
ai-confirm-rejected = Rejected
ai-diff-more = Lines not shown: { $count }
ai-usage-input = { $count } in
ai-usage-output = { $count } out
ai-usage-cache = { $percent } % cached
ai-stopped = Stopped
ai-copied = Copied
ai-scroll-bottom = Scroll to bottom
ai-scroll-bottom-new = Scroll to bottom: new text below
ai-action-needs-selection = Select text first
ai-action-needs-note = Open a note first
ai-needs-key = Add an API key in the settings
ai-actions-running = Wait for the answer to end
ai-masking-active = Secrets are masked before sending
ai-mention-empty = No matching note
ai-actions-target-selection = Applies to the selection
ai-actions-target-note = Applies to the note
ai-actions-no-target = Open a note or select text to use actions.
ai-error-missing-key = No { $provider } API key is set. Add one in Settings › AI.
ai-error-invalid-key = { $provider } refused the API key (401). Check it in Settings › AI.
ai-error-balance = Your { $provider } balance is too low (402). Top up the account, then try again.
ai-error-rate-limit = { $provider } received too many requests (429). Try again in { $seconds } s.
ai-error-bad-request = { $provider } rejected the request ({ $status }): { $message }
ai-error-server = { $provider } is unavailable ({ $status }). Try again in a moment.
ai-error-network = Focus cannot reach { $provider }: { $message }. Check the connection, then try again.
ai-error-protocol = { $provider } sent a response Focus cannot read: { $message }. Try again.
ai-error-refused = The model declined this request (category: { $category }). Rephrase it or choose another model.
ai-error-no-model = No model is set for { $provider }, and none can be chosen for you. Set the model in Settings › AI, then try again.
ai-tools-disabled = This model does not accept tools: Focus sent the request again without them.
ai-link-unresolved = No note matches { $target }.
ai-mcp-failed = The MCP server { $server } did not start: { $reason }
ai-usage-conversation = This conversation: { $tokens } tokens, estimated cost { $cost }.
ai-ops-reminder = Ops mode: question, context and tool results are sent to { $provider }.
ai-ops-reminder-detail = Ops mode: your question, the context chips and the results of the tools go to { $provider }. Passwords, hashes, keys and tokens are masked first; IP addresses are masked only when that option is on.
ai-ops-reminder-local = Ops mode: question, context and tool results go to { $provider } on this machine ({ $url }) and do not leave it.
ai-ops-reminder-local-detail = Ops mode: your question, the context chips and the results of the tools go to { $provider } on this machine ({ $url }) and do not leave it. Nothing is sent over the network.
ai-provider-local = The request stays on this machine (address { $url }).
