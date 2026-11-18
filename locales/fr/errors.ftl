errors-core-io = Accès impossible à { $path } : { $detail }. Vérifiez que le fichier existe et que vous pouvez y écrire.
errors-core-outside-vault = Le chemin { $path } est hors du coffre. Choisissez un emplacement dans le coffre.
errors-core-already-exists = { $path } existe déjà. Choisissez un autre nom.
errors-core-unknown-note = Cette note n'existe plus dans le coffre. Actualisez l'arbre.
errors-core-conflict = { $path } a changé sur le disque depuis sa dernière lecture. Rechargez-le ou gardez votre version.
errors-core-other = { $message }

errors-search-index-build = L'index de recherche n'a pas pu être construit : { $detail }
errors-search-index-write = L'index de recherche n'a pas pu être enregistré : { $detail }

errors-ai-missing-key = Aucune clé API. Ajoutez votre clé DeepSeek dans Réglages, section IA.
errors-ai-invalid-key = Clé API refusée par DeepSeek (401). Vérifiez-la dans Réglages, section IA.
errors-ai-insufficient-balance = Le solde DeepSeek est insuffisant (402). Rechargez le compte, puis réessayez.
errors-ai-rate-limited = Limite de débit atteinte (429). Réessayez dans { $retry_after_secs } s.
errors-ai-bad-request = Requête refusée par DeepSeek ({ $status }) : { $message }
errors-ai-server = Le serveur DeepSeek est indisponible ({ $status }). Réessayez dans un instant.
errors-ai-network = Erreur réseau : { $detail }. Vérifiez votre connexion.
errors-ai-protocol = La réponse est illisible : { $detail }
errors-ai-cancelled = Annulé

errors-ai-mcp-invalid-config = Serveur MCP « { $server } » : configuration invalide : { $reason }
errors-ai-mcp-spawn = Serveur MCP « { $server } » : lancement de `{ $command }` impossible : { $reason }. Vérifiez la commande dans Réglages, section IA.
errors-ai-mcp-timeout = Serveur MCP « { $server } » : pas de réponse à `{ $method }` après { $secs } s. { $stderr }
errors-ai-mcp-exited = Serveur MCP « { $server } » : le processus s'est arrêté. { $detail }
errors-ai-mcp-protocol = Serveur MCP « { $server } » : { $reason }
errors-ai-mcp-cancelled = Annulé

errors-ops-io = Accès impossible à { $path } : { $detail }. Vérifiez que le fichier existe et que vous pouvez y écrire.
errors-ops-already-exists = L'engagement { $path } existe déjà. Choisissez un autre nom.
errors-ops-parse = { $file } est illisible : { $message }
errors-ops-invalid = { $message }
errors-ops-conflict = { $path } a changé sur le disque depuis sa lecture. Rechargez-le avant d'enregistrer.

errors-md-unsupported = Ce type de diagramme n'est pas pris en charge : { $kind }. La source est affichée à la place.
errors-md-syntax = Syntaxe Mermaid invalide : { $detail }
errors-md-render = Le diagramme n'a pas pu être dessiné : { $detail }

errors-mcp-not-running = L'application Focus n'est pas ouverte sur ce coffre. Ouvrez-la, puis réessayez.
errors-mcp-failed = { $message }

errors-vault-open = Le coffre n'a pas pu être ouvert : { $detail }
errors-folder-picker = Le sélecteur de dossier a échoué : { $detail }
errors-theme = Le thème n'a pas pu être appliqué : { $detail }
errors-settings-read = Le fichier de réglages est illisible : { $detail }. Les valeurs par défaut sont utilisées.
errors-settings-write = Les réglages n'ont pas pu être enregistrés : { $detail }
errors-update-network = Le serveur des versions n'a pas répondu. Vérifiez la connexion, puis réessayez.
errors-update-unsigned = La dernière version n'est pas signée. Rien n'a été installé.
errors-update-signature = La version n'est pas signée avec la clé de publication de Focus. Rien n'a été installé.
errors-update-damaged = Le téléchargement ne correspond pas à la version signée. Rien n'a été installé.
errors-update-missing = Cette version n'a pas de fichier pour cette installation. Récupérez-le sur la page de la version.
errors-update-write = Focus n'a pas pu écrire dans { $folder } ({ $detail }). Récupérez la nouvelle version sur la page de la version.
errors-translation-file = Fichier de traduction ignoré ({ $file }) : { $detail }
errors-ai-keystore = La clé d'API n'a pas pu être enregistrée dans le trousseau du système : { $detail }
errors-copy-note = La note n'a pas pu être copiée : { $detail }
errors-export-report = Le rapport n'a pas pu être exporté : { $detail }
clipboard-timeout = Le presse-papiers n'a pas répondu en 3 secondes.
clipboard-too-large = Le presse-papiers contient plus de { $size } : rien n'est collé.
clipboard-failed = { $tool } n'a pas pu lire le presse-papiers.
clipboard-start = { $tool } n'a pas pu être lancé pour lire le presse-papiers.
