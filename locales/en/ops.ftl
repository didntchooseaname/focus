ops-title = Ops
ops-new-engagement = New engagement
ops-switch-engagement = Switch engagement
ops-no-engagement = No active engagement
ops-dashboard = Engagement dashboard
ops-set-target = Set as target
ops-target = Target
ops-variables = Variables
ops-report-export-sysreptor = Export to SysReptor
ops-sysreptor-written = SysReptor project written to { $path }. Import it in SysReptor as a project. What was left out or adjusted:
ops-sysreptor-failed = The SysReptor export failed: { $detail }
ops-proofs-name = Proof name (local.txt, proof.txt...)
ops-proofs-value = Value
ops-proofs-machine = Exam machine
ops-proofs-title = Proofs ({ $count })
ops-proofs-add = Add the proof
ops-timer = Timer
ops-timer-start = Start the timer
ops-timer-pause = Pause
ops-timer-resume = Resume
ops-timer-stop = Stop
ops-timer-stopped = Stopped after { $duration } worked.
ops-trash = Move to the trash
ops-trash-title = Move { $name } to the trash?
ops-trash-confirm-vault = The folder of { $name } goes to the vault trash (the .trash folder), where you can restore it. It is not deleted.
ops-trash-confirm-system = The folder of { $name } goes to the system trash, where you can restore it. It is not deleted.
ops-trash-do = Move to the trash
ops-var-credential-pick = Choose from the credentials
ops-var-credential-linked = Follows a credential
ops-var-credential-unlink = Stop following the credential
ops-credentials-use = Use this credential for the variables
ops-credentials-used = Variables set: { $names }.
ops-commands-show-secrets = Show secrets in the commands
ops-commands-hide-secrets = Hide secrets in the commands
ops-substitute-hint = Replace variables in commands
ops-context = Context
ops-copy-resolved = Copy resolved
ops-copy-raw = Copy raw
ops-open-source-note = Open the source note

ops-kind-external-pentest = External pentest
ops-kind-internal-pentest = Internal pentest
ops-kind-web-pentest = Web pentest
ops-kind-red-team = Red team
ops-kind-lab = Lab
ops-kind-exam = Exam

ops-status-planning = Preparation
ops-status-in-progress = In progress
ops-status-reporting = Report
ops-status-done = Done

ops-host-status-new = New
ops-host-status-enum = Enumerated
ops-host-status-foothold = Foothold
ops-host-status-user = User
ops-host-status-owned = Owned

ops-severity-info = Info
ops-severity-low = Low
ops-severity-medium = Medium
ops-severity-high = High
ops-severity-critical = Critical

ops-finding-status-draft = Draft
ops-finding-status-confirmed = Confirmed
ops-finding-status-reported = Reported
ops-finding-status-fixed = Fixed
ops-finding-status-accepted = Accepted

ops-section-overview = Overview
ops-section-journal = Journal
ops-section-hosts = Hosts
ops-section-findings = Findings
ops-section-credentials = Credentials
ops-section-checklists = Checklists
ops-section-commands = Command library
ops-section-exam = Exam
ops-section-report = Report
ops-section-evidence = Evidence
ops-section-scope = Scope

ops-journal-add = Add an entry
ops-journal-placeholder = What happened, on which target
ops-journal-filter = Filter the journal
ops-journal-export-csv = Export as CSV
ops-journal-count =
    { $count ->
        [one] { $count } entry
       *[other] { $count } entries
    }

ops-hosts-column-address = Address
ops-hosts-column-name = Name
ops-hosts-column-os = System
ops-hosts-column-status = Status
ops-hosts-column-ports = Ports
ops-hosts-import-nmap = Import nmap scan
ops-hosts-count =
    { $count ->
        [one] { $count } host
       *[other] { $count } hosts
    }

ops-finding-new = New finding
ops-finding-severity = Severity
ops-finding-status = Status
ops-finding-assets = Affected assets
ops-finding-cwe = CWE
ops-finding-cwe-placeholder = Add a weakness: number or name
ops-finding-cwe-remove = Remove this weakness
ops-finding-cwe-filter = Filter by CWE
ops-finding-cwe-invalid = { $value } is not a CWE identifier or a name in the catalog.
ops-finding-cwe-none = No weakness matches
ops-finding-body = Text
ops-finding-body-placeholder = Write the finding here (Markdown)
ops-finding-body-save = Save text
ops-finding-body-reload = Reload from the note
ops-finding-preview = Preview
ops-finding-attack = ATT&CK techniques
ops-finding-score = CVSS score
ops-finding-vector = CVSS vector
ops-finding-count =
    { $count ->
        [one] { $count } finding
       *[other] { $count } findings
    }

ops-cvss-calculator = CVSS calculator
ops-cvss-version = Version { $version }
ops-cvss-invalid-vector = This CVSS vector is not valid.

ops-credentials-column-user = User
ops-credentials-column-secret = Secret
ops-credentials-column-source = Source
ops-credentials-add = Add a credential
ops-credentials-reveal = Reveal
ops-credentials-hide = Hide

ops-checklist-progress = { $done } of { $total } done
ops-checklist-next-step = Next step
ops-checklist-from-note = Use a note as a template

ops-exam-profile = Exam profile
ops-exam-time-left = { $duration } left
ops-exam-time-over = Time is over
ops-exam-alert = Less than { $minutes } min left
ops-exam-points =
    { $points ->
        [one] { $points } point
       *[other] { $points } points
    }
ops-exam-phase-practical = Practical phase
ops-exam-phase-report = Report phase
ops-exam-rules = Rules

ops-report-build = Build the report
ops-report-export-html = Export as HTML
ops-report-export-pdf = Export as PDF
ops-evidence-paste = Paste a screenshot as evidence
ops-reminder-sent-data = What leaves your machine

ops-stat-hosts = Hosts
ops-stat-owned = Owned
ops-stat-findings = Findings
ops-stat-critical = Critical
ops-stat-checklist = Checklist
ops-latest-events = Latest events

ops-exam-oscp-rule-1 = The exam is proctored.
ops-exam-oscp-rule-2 = Proof files local.txt and proof.txt: read in an interactive shell on the target with type or cat, from their original location; a web shell is not enough. Each proof appears in a screenshot of the report.
ops-exam-oscp-rule-3 = On Windows: a SYSTEM, Administrator or administrator-level shell; on Linux: a root shell.
ops-exam-oscp-rule-4 = Metasploit and Meterpreter: one target machine of your choice, locked in at the first use.
ops-exam-oscp-rule-5 = Forbidden: spoofing (IP, ARP, DNS, NBNS), commercial tools, automatic exploitation, mass vulnerability scanners, AI assistants and language models.
ops-exam-oscp-rule-6 = Report: every attack documented step by step, delivered within 24 h after the exam; delivery is final.
ops-exam-osep-rule-1 = The exam is proctored.
ops-exam-osep-rule-2 = Proof files local.txt and proof.txt: read in an interactive shell on the target; a web shell or an RDP session is not enough.
ops-exam-osep-rule-3 = Report as a PDF archived in a .7z, scripts and proofs of concept copied as text into the PDF; delivery is final.
ops-exam-osep-rule-4 = A proof missing from the screenshot gives zero points for the target.
ops-exam-oswe-rule-1 = The exam is proctored.
ops-exam-oswe-rule-2 = Report as a PDF archived in a .7z, proof of concept script pasted as text (or as base64) into the PDF; delivery is final.
ops-exam-oswe-rule-3 = Missing proof or documentation can reduce the points of an objective, or cancel them.
ops-exam-osed-rule-1 = The exam is proctored.
ops-exam-osed-rule-2 = Three tasks; points are awarded only for a fully completed task.
ops-exam-osed-rule-3 = One proof per target, submitted in the control panel and shown in a screenshot.
ops-exam-osed-rule-4 = Report describing the discovery and exploitation of each solved task; delivery is final.
ops-exam-crtp-rule-1 = At the end of the exam: detailed solutions to the challenges and practical mitigations to deliver.
ops-exam-crtp-rule-2 = One attempt included; unfair practices are sanctioned with a six month cool-down.
ops-exam-crte-rule-1 = At the end of the exam: detailed solutions to the challenges and practical mitigations to deliver.
ops-exam-crte-rule-2 = Environment with several domains and forests.
ops-exam-crto-rule-1 = 48 hours of lab runtime, usable within a 4 day window.
ops-exam-crto-rule-2 = One flag per machine, submitted on the scoreboard: 6 out of 8 to pass. Scoring is in flags, not points.
ops-exam-pnpt-rule-1 = From open source intelligence to compromising the exam domain controller.
ops-exam-pnpt-rule-2 = Professional written report, then a 15 minute oral debrief in front of the assessors.
ops-exam-cpts-rule-1 = Commercial grade report to upload on the exam page within 10 days of entering the exam.
ops-exam-cpts-rule-2 = Several flags to submit on the exam page; a minimum number of points is required together with the report.
ops-exam-cpts-rule-3 = A report delivered after the deadline makes the exam voucher expire, with no second attempt.
ops-exam-bscp-rule-1 = Two applications, three stages each: access an account, access the admin interface, read /home/carlos/secret.
ops-exam-bscp-rule-2 = A Burp project file must be used during the whole exam and submitted for analysis.
ops-exam-bscp-rule-3 = No outside help; do not share the exam addresses.
ops-exam-ejpt-rule-1 = Minimum score of 70 % overall and a minimum in each domain.
ops-exam-ejpt-rule-2 = Multiple choice questions and labs; graded automatically.
ops-exam-ecppt-rule-1 = Minimum score of 70 % overall; graded automatically.
ops-exam-ecppt-rule-2 = Voucher valid 180 days, one free retake within 14 days.
ops-exam-oscp-machine-1 = Standalone machine 1
ops-exam-oscp-machine-2 = Standalone machine 2
ops-exam-oscp-machine-3 = Standalone machine 3
ops-exam-oscp-machine-4 = Active Directory set, machine 1
ops-exam-oscp-machine-5 = Active Directory set, machine 2
ops-exam-oscp-machine-6 = Active Directory set, machine 3
ops-exam-milestone-initial-access = Initial access (local.txt)
ops-exam-milestone-privilege-escalation = Privilege escalation (proof.txt)

ops-cvss-metric-av = Attack vector
ops-cvss-metric-ac = Attack complexity
ops-cvss-metric-at = Attack requirements
ops-cvss-metric-pr = Privileges required
ops-cvss-metric-ui = User interaction
ops-cvss-metric-s = Scope
ops-cvss-metric-c = Confidentiality
ops-cvss-metric-i = Integrity
ops-cvss-metric-a = Availability
ops-cvss-metric-vc = Vulnerable system confidentiality
ops-cvss-metric-vi = Vulnerable system integrity
ops-cvss-metric-va = Vulnerable system availability
ops-cvss-metric-sc = Subsequent system confidentiality
ops-cvss-metric-si = Subsequent system integrity
ops-cvss-metric-sa = Subsequent system availability
ops-cvss-value-av-n = Network
ops-cvss-value-av-a = Adjacent
ops-cvss-value-av-l = Local
ops-cvss-value-av-p = Physical
ops-cvss-value-ac-l = Low
ops-cvss-value-ac-h = High
ops-cvss-value-at-n = None
ops-cvss-value-at-p = Present
ops-cvss-value-pr-n = None
ops-cvss-value-pr-l = Low
ops-cvss-value-pr-h = High
ops-cvss-value-ui-n = None
ops-cvss-value-ui-r = Required
ops-cvss-value-ui-p = Passive
ops-cvss-value-ui-a = Active
ops-cvss-value-s-u = Unchanged
ops-cvss-value-s-c = Changed
ops-cvss-value-c-h = High
ops-cvss-value-c-l = Low
ops-cvss-value-c-n = None
ops-cvss-value-i-h = High
ops-cvss-value-i-l = Low
ops-cvss-value-i-n = None
ops-cvss-value-a-h = High
ops-cvss-value-a-l = Low
ops-cvss-value-a-n = None
ops-cvss-value-vc-h = High
ops-cvss-value-vc-l = Low
ops-cvss-value-vc-n = None
ops-cvss-value-vi-h = High
ops-cvss-value-vi-l = Low
ops-cvss-value-vi-n = None
ops-cvss-value-va-h = High
ops-cvss-value-va-l = Low
ops-cvss-value-va-n = None
ops-cvss-value-sc-h = High
ops-cvss-value-sc-l = Low
ops-cvss-value-sc-n = None
ops-cvss-value-si-h = High
ops-cvss-value-si-l = Low
ops-cvss-value-si-n = None
ops-cvss-value-sa-h = High
ops-cvss-value-sa-l = Low
ops-cvss-value-sa-n = None

ops-var-rhost-desc = Target address
ops-var-lhost-desc = Local machine address
ops-var-lport-desc = Local listening port
ops-var-domain-desc = Domain
ops-var-user-desc = User name
ops-var-pass-desc = Password
ops-var-hash-desc = Password hash
ops-var-dc-desc = Domain controller
ops-var-dc_host-desc = Domain controller host name
ops-var-domain_sid-desc = Domain SID
ops-var-url-desc = Target web address
ops-var-wordlist-desc = Word list file
ops-var-hostname-desc = Target name
ops-var-interface-desc = Network interface

ops-checklist-mission-label = Engagement workflow
ops-checklist-exam-label = Exam workflow

ops-cancel = Cancel
ops-close = Close
ops-commands-count = Commands: { $count }
ops-commands-empty = No command found. Focus reads the shell code blocks of your own notes, in the folders set in the settings.
ops-commands-journaled = Command added to the journal.
ops-commands-no-match = No command matches this search
ops-commands-search = Search by tool, command or heading
ops-commands-to-journal = Add to the journal
ops-copied = Copied
ops-credentials-column-domain = Domain
ops-credentials-column-kind = Type
ops-credentials-copy = Copy the secret
ops-credentials-count =
    { $count ->
        [one] { $count } credential
       *[other] { $count } credentials
    }
ops-credentials-duplicate = This credential is already recorded.
ops-credentials-empty = No credential yet. Add one above.
ops-cvss-apply = Apply to the selected finding
ops-exam-points-label = Points
ops-exam-points-of = { $points } of { $pass } to pass ({ $total } in all)
ops-exam-points-pass = { $points } of { $pass } to pass
ops-filter-all = All
ops-finding-create = Create
ops-finding-title-placeholder = Title of the finding
ops-finding-vector-copy = Copy the vector
ops-findings-empty = No finding to show. Create one with New finding.
ops-hosts-empty = No host yet. Import an nmap scan to fill this table.
ops-hosts-import = Import
ops-hosts-import-hint = Paste the output of an nmap scan (XML, grepable or normal).
ops-hosts-import-report = Created: { $created }, updated: { $updated }, services read: { $services }.
ops-hosts-paste-placeholder = Paste the scan here
ops-journal-all-targets = All targets
ops-journal-empty = The journal is empty. Add an entry from the Ops bar.
ops-journal-no-match = No entry matches this filter
ops-loading = Reading the engagement…
ops-new-client = Client
ops-new-create = Create the engagement
ops-new-kind = Type of engagement
ops-new-name = Name
ops-new-name-required = Give the engagement a name.
ops-new-scope-placeholder = Addresses or domains
ops-new-template = Checklist template
ops-new-template-default = Default for this type
ops-no-target = No target
ops-open-note = Open the note
ops-exam-rules-count = Rules: { $count }

ops-credentials-column-valid = Works on
ops-host-cited-none = Nothing cites this host yet.
ops-host-cited-findings = Findings that cite this host
ops-host-cited-credentials = Credentials that cite this host
ops-journal-copy-link = Copy the link to this entry
ops-report-author = Author
ops-report-empty = Build the report to preview it here.
ops-report-include-credentials = Credentials
ops-report-include-journal = Journal
ops-report-mask-secrets = Mask secrets
ops-report-preview = Preview
ops-report-secrets-warning = The report will contain secrets in clear text.
ops-report-write = Write Report.md
ops-report-written = { $file } written.
ops-new-note-search = Search a note with checkboxes
ops-var-add = Add the variable
ops-var-invalid-name = A variable name uses letters, digits and underscores, and does not start with a digit.
ops-var-name-placeholder = NAME
ops-var-value-placeholder = Value

ops-finding-sheet = Finding
ops-finding-sheet-empty = Select a finding to read its note.

ops-exam-start = Start the exam timer
ops-exam-not-started = { $minutes } minutes, not started
ops-finding-edit = Edit
ops-finding-edit-done = Done
ops-finding-techniques = ATT&CK techniques
ops-finding-asset-placeholder = Add an asset (host or address)
ops-finding-technique-placeholder = Add a technique (T1059.001)
ops-finding-asset-remove = Remove this asset
ops-finding-technique-remove = Remove this technique
ops-finding-technique-invalid = “{ $value }” is not an ATT&CK technique identifier.
ops-filter-severity = Severity: { $value }
ops-filter-status = Status: { $value }
ops-scope = Scope
ops-scope-placeholder = Addresses or domains, Enter to save
ops-hosts-import-file = Choose a scan file…
ops-sort-severity = Sorted by severity
ops-sort-creation = Newest first

ops-targets-address = Address, range or name
ops-targets-hostname = Host name
ops-targets-os = System
ops-targets-tags = Tags, comma separated
ops-targets-paste-placeholder = 10.0.0.5
    10.0.1.0/24
    dc01.corp.local
ops-targets-add-button = Add targets
ops-targets-manual = Add one target
ops-targets-add = Add
ops-targets-list = Paste a list or read a file
ops-targets-hint = One target per line: an address, a range (CIDR) or a name, then optionally a host name. # starts a comment. A CSV with a header row is read by its headers: ip or address, hostname or name, os, tags, notes (comma, semicolon or tab).
ops-targets-preview-button = Preview
ops-targets-file = Choose a text or CSV file…
ops-targets-known = This target is already in the engagement.
ops-targets-created = Targets created: { $count }.
ops-targets-preview = To create: { $create }. Already present: { $known }. Refused: { $refused }.
ops-targets-more = and { $count } more
ops-targets-known-title = Already present, left as they are
ops-targets-refused-title = Refused
ops-targets-refused-line = Line { $line }: { $text } ({ $reason })
ops-targets-confirm = Create the targets ({ $count })
ops-targets-reason-address = not an address, a range or a name
ops-targets-reason-name = not a host name
ops-targets-reason-extra = text after the address and the name
ops-targets-reason-long = too long
ops-targets-reason-repeated = listed twice
ops-targets-reason-many = too many targets in one list
ops-targets-reason-none = no address
