ai-title = IA
ai-input-placeholder = Posez une question sur vos notes
ai-send = Envoyer
ai-stop = Arrêter
ai-retry = Réessayer
ai-copy = Copier
ai-insert-at-cursor = Insérer au curseur
ai-save-as-note = Enregistrer comme note
ai-new-conversation = Nouvelle conversation
ai-history = Historique
ai-history-empty = Aucune conversation pour l'instant.
ai-reasoning = Raisonnement
ai-reasoning-show = Afficher le raisonnement
ai-reasoning-hide = Masquer le raisonnement
ai-generating = Génération en cours…
ai-tool-call = Appel d'outil : { $name }
ai-tool-search-notes = Recherche dans les notes
ai-tool-read-note = Lecture d'une note
ai-tool-list-folder = Liste d'un dossier
ai-tool-get-backlinks = Lecture des rétroliens
ai-tool-get-outline = Lecture d'un plan
ai-tool-list-tags = Liste des tags
ai-confirm-title = Confirmer cette modification
ai-confirm-apply = Appliquer
ai-confirm-reject = Refuser
ai-confirm-always-allow = Toujours autoriser cet outil pour { $server }
ai-diff-title = Différences
ai-context-note = Note courante : { $path }
ai-context-selection = Sélection
ai-context-added = Ajoutée avec @ : { $path }
ai-context-remove = Retirer du contexte
ai-tokens-approx = ~{ $count }
ai-tokens-approx-thousands = ~{ $count }k
ai-context-tokens =
    { $count ->
        [one] Environ { $count } jeton
       *[other] Environ { $count } jetons
    }
ai-usage-tokens =
    { $count ->
        [one] { $count } jeton
       *[other] { $count } jetons
    }
ai-usage-cached = { $percent } % servis par le cache
ai-usage-cost = Coût estimé { $cost }
ai-balance = Solde : { $amount }
ai-redaction-notice =
    { $count ->
        [one] { $count } secret masqué avant l'envoi
       *[other] { $count } secrets masqués avant l'envoi
    }
ai-connection-ok = La connexion fonctionne. Solde : { $amount }.
ai-connection-testing = Test de la connexion…
ai-mcp-state-running = En cours
ai-mcp-state-stopped = Arrêté
ai-mcp-state-error = Erreur
ai-mcp-tools =
    { $count ->
        [one] { $count } outil
       *[other] { $count } outils
    }
ai-actions-edit = Modifier les actions
ai-action-duplicate = Dupliquer
ai-action-hide = Masquer
ai-action-restore = Rétablir
ai-action-builtin = Intégrée
ai-action-ops-only = Mode Ops seulement

ai-action-summarize-label = Résumer
ai-action-summarize-desc = Idée centrale et points clés de la sélection.
ai-action-refine-label = Affiner
ai-action-refine-desc = Améliore la clarté et la structure sans changer le fond.
ai-action-rephrase-label = Reformuler
ai-action-rephrase-desc = Dit la même chose avec d'autres mots.
ai-action-correct-label = Corriger
ai-action-correct-desc = Corrige l'orthographe, la grammaire et la ponctuation.
ai-action-translate-fr-label = Traduire en français
ai-action-translate-fr-desc = Garde le Markdown, le code et les liens tels quels.
ai-action-translate-en-label = Traduire en anglais
ai-action-translate-en-desc = Garde le Markdown, le code et les liens tels quels.
ai-action-continue-label = Continuer
ai-action-continue-desc = Écrit le paragraphe suivant dans le même ton.
ai-action-explain-label = Expliquer
ai-action-explain-desc = Explique le texte à un lecteur qui connaît le domaine.
ai-action-tasks-label = Extraire les tâches
ai-action-tasks-desc = Transforme le texte en liste de tâches Markdown.
ai-action-title-tags-label = Proposer titre et tags
ai-action-title-tags-desc = Un titre court et trois à six tags pour la note.
ai-action-mermaid-label = Générer un diagramme Mermaid
ai-action-mermaid-desc = Dessine la structure du texte sous forme de diagramme.
ai-action-note-from-outline-label = Générer une note depuis un plan
ai-action-note-from-outline-desc = Rédige une note complète à partir du plan sélectionné.
ai-action-ops-finding-label = Rédiger un constat
ai-action-ops-finding-desc = Description, impact, preuve, remédiation et références à partir de vos notes.
ai-action-ops-exec-summary-label = Résumé exécutif
ai-action-ops-exec-summary-desc = Résumé de l'engagement pour un lecteur non technique.
ai-action-ops-next-enum-label = Prochaines étapes d'énumération
ai-action-ops-next-enum-desc = Prochaines étapes, de la plus rentable à la moins rentable.

ai-output-chat = Afficher dans le fil
ai-output-replace = Remplacer le texte
ai-output-insert = Insérer dessous
ai-output-note = Créer une note

ai-model = Modèle
ai-actions = Actions
ai-add-context = Ajouter une note au contexte
ai-empty-title = Interrogez vos notes
ai-empty-hint = Ajoutez des notes au contexte avec @, ou lancez une action sur la note courante.
ai-no-key-title = Aucune clé API
ai-no-key-body = Ajoutez votre clé API { $provider } dans les réglages pour poser des questions et lancer des actions.
ai-no-key-open = Ouvrir les réglages
ai-history-untitled = Conversation sans titre
ai-history-delete = Supprimer la conversation
ai-tool-refused = Refusé
ai-tool-failed = Échec
ai-tool-no-output = L'outil n'a rien renvoyé.
ai-proposal-replace = Remplacement proposé
ai-confirm-applied = Appliqué
ai-confirm-rejected = Refusé
ai-diff-more = Lignes non affichées : { $count }
ai-usage-input = { $count } en entrée
ai-usage-output = { $count } en sortie
ai-usage-cache = { $percent } % en cache
ai-stopped = Arrêté
ai-copied = Copié
ai-scroll-bottom = Aller en bas
ai-scroll-bottom-new = Aller en bas : du nouveau texte plus bas
ai-action-needs-selection = Sélectionnez du texte d'abord
ai-action-needs-note = Ouvrez d'abord une note
ai-needs-key = Ajoutez une clé d'API dans les réglages
ai-actions-running = Attendez la fin de la réponse
ai-masking-active = Les secrets sont masqués avant l'envoi
ai-mention-empty = Aucune note correspondante
ai-actions-target-selection = S'applique à la sélection
ai-actions-target-note = S'applique à la note
ai-actions-no-target = Ouvrez une note ou sélectionnez du texte pour utiliser les actions.
ai-error-missing-key = Aucune clé API { $provider } n'est définie. Ajoutez-en une dans Réglages › IA.
ai-error-invalid-key = { $provider } a refusé la clé API (401). Vérifiez-la dans Réglages › IA.
ai-error-balance = Le solde { $provider } est insuffisant (402). Rechargez le compte puis réessayez.
ai-error-rate-limit = { $provider } a reçu trop de requêtes (429). Réessayez dans { $seconds } s.
ai-error-bad-request = { $provider } a refusé la requête ({ $status }) : { $message }
ai-error-server = { $provider } est indisponible ({ $status }). Réessayez dans un instant.
ai-error-network = Focus ne joint pas { $provider } : { $message }. Vérifiez la connexion puis réessayez.
ai-error-protocol = { $provider } a envoyé une réponse que Focus ne peut pas lire : { $message }. Réessayez.
ai-error-refused = Le modèle a refusé cette demande (catégorie : { $category }). Reformulez-la ou choisissez un autre modèle.
ai-error-no-model = Aucun modèle n'est réglé pour { $provider } et aucun ne peut être choisi à votre place. Réglez le modèle dans Réglages › IA, puis réessayez.
ai-tools-disabled = Ce modèle n'accepte pas les outils : Focus a renvoyé la demande sans eux.
ai-link-unresolved = Aucune note ne correspond à { $target }.
ai-mcp-failed = Le serveur MCP { $server } n'a pas démarré : { $reason }
ai-usage-conversation = Cette conversation : { $tokens } jetons, coût estimé { $cost }.
ai-ops-reminder = Mode Ops : question, contexte et résultats d'outils partent chez { $provider }.
ai-ops-reminder-detail = Mode Ops : votre question, les pastilles de contexte et les résultats des outils partent chez { $provider }. Mots de passe, empreintes, clés et jetons sont masqués d'abord ; les adresses IP ne le sont que si l'option est active.
ai-ops-reminder-local = Mode Ops : la question, le contexte et les résultats d'outils vont à { $provider } sur cette machine ({ $url }) et n'en sortent pas.
ai-ops-reminder-local-detail = Mode Ops : votre question, les pastilles de contexte et les résultats des outils vont à { $provider } sur cette machine ({ $url }) et n'en sortent pas. Rien n'est envoyé sur le réseau.
ai-provider-local = La requête reste sur cette machine (adresse { $url }).
