history-title = Versions of { $note }
history-close = Close
history-loading = Reading the versions…
history-empty = No version of this note yet
history-empty-hint = Focus keeps the content a write replaces, at most every 5 minutes. Edit the note, then come back.
history-off-hint = The history is off. Turn it on in Settings, Files and links.
history-kind-edit = Edit
history-kind-bulk = Change across notes
history-kind-restore = Before a restore
history-kind-external = Written by another program
history-kind-unsaved = Your unsaved text
history-kind-reload = Before a reload from disk
history-copy-syncthing = Conflict copy from Syncthing
history-copy-syncthing-device = Conflict copy from Syncthing, device { $device }
history-copy-obsidian = Conflict copy from Obsidian Sync
history-copy-obsidian-device = Conflict copy from Obsidian Sync, { $device }
history-tab-changes = Changes
history-tab-rendered = Rendered
history-same = This version is the same as the current text.
history-copy-same = This copy is the same as the current text.
history-count-added = added
history-count-removed = removed
history-copy = Copy
history-restore = Restore
history-copied = Version copied
history-restored = Version restored
history-delete-copy = Delete the copy
history-copy-text-copied = Text of the copy copied
history-copy-restored = Copy restored
history-copy-deleted = Copy deleted
history-folded =
    { $count ->
        [one] { $count } unchanged line
       *[other] { $count } unchanged lines
    }
history-error-unavailable = The history of this vault is not available. Restart Focus or check the data folder.
history-error-unknown-note = The note is no longer in the vault.
history-error-gone = This version no longer exists. The list was reloaded.
history-error-damaged = The stored copy of this version is damaged and cannot be read.
history-error-io = The history could not be read or written ({ $path }). Check its permissions.
history-error-conflict = The note changed on disk. The list and the changes were reloaded: check them, then restore again.
history-error-other = The note could not be written: { $detail }
