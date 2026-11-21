ops-title = Ops
ops-new-engagement = Nouvel engagement
ops-switch-engagement = Changer d'engagement
ops-no-engagement = Aucun engagement actif
ops-dashboard = Tableau de bord de l'engagement
ops-set-target = Définir comme cible
ops-target = Cible
ops-variables = Variables
ops-report-export-sysreptor = Exporter vers SysReptor
ops-sysreptor-written = Projet SysReptor écrit dans { $path }. Importez-le dans SysReptor comme un projet. Ce qui a été laissé de côté ou ajusté :
ops-sysreptor-failed = L'export vers SysReptor a échoué : { $detail }
ops-proofs-name = Nom de la preuve (local.txt, proof.txt...)
ops-proofs-value = Valeur
ops-proofs-machine = Machine de l'examen
ops-proofs-title = Preuves ({ $count })
ops-proofs-add = Ajouter la preuve
ops-timer = Chronomètre
ops-timer-start = Démarrer le chronomètre
ops-timer-pause = Pause
ops-timer-resume = Reprendre
ops-timer-stop = Arrêter
ops-timer-stopped = Arrêté après { $duration } de travail.
ops-trash = Mettre à la corbeille
ops-trash-title = Mettre { $name } à la corbeille ?
ops-trash-confirm-vault = Le dossier de { $name } va dans la corbeille du coffre (le dossier .trash), d'où vous pouvez le restaurer. Il n'est pas supprimé.
ops-trash-confirm-system = Le dossier de { $name } va dans la corbeille du système, d'où vous pouvez le restaurer. Il n'est pas supprimé.
ops-trash-do = Mettre à la corbeille
ops-var-credential-pick = Choisir parmi les identifiants
ops-var-credential-linked = Suit un identifiant
ops-var-credential-unlink = Ne plus suivre l'identifiant
ops-credentials-use = Utiliser cet identifiant pour les variables
ops-credentials-used = Variables réglées : { $names }.
ops-commands-show-secrets = Afficher les secrets dans les commandes
ops-commands-hide-secrets = Masquer les secrets dans les commandes
ops-substitute-hint = Remplacer les variables dans les commandes
ops-context = Contexte
ops-copy-resolved = Copier résolu
ops-copy-raw = Copier brut
ops-open-source-note = Ouvrir la note source

ops-kind-external-pentest = Test d'intrusion externe
ops-kind-internal-pentest = Test d'intrusion interne
ops-kind-web-pentest = Test d'intrusion web
ops-kind-red-team = Red team
ops-kind-lab = Labo
ops-kind-exam = Examen

ops-status-planning = Préparation
ops-status-in-progress = En cours
ops-status-reporting = Rapport
ops-status-done = Terminé

ops-host-status-new = Nouveau
ops-host-status-enum = Énuméré
ops-host-status-foothold = Accès initial
ops-host-status-user = Utilisateur
ops-host-status-owned = Compromis

ops-severity-info = Info
ops-severity-low = Faible
ops-severity-medium = Moyenne
ops-severity-high = Haute
ops-severity-critical = Critique

ops-finding-status-draft = Brouillon
ops-finding-status-confirmed = Confirmé
ops-finding-status-reported = Rapporté
ops-finding-status-fixed = Corrigé
ops-finding-status-accepted = Accepté

ops-section-overview = Vue d'ensemble
ops-section-journal = Journal
ops-section-hosts = Hôtes
ops-section-findings = Constats
ops-section-credentials = Identifiants
ops-section-checklists = Listes de contrôle
ops-section-commands = Bibliothèque de commandes
ops-section-exam = Examen
ops-section-report = Rapport
ops-section-evidence = Preuves
ops-section-scope = Périmètre

ops-journal-add = Ajouter une entrée
ops-journal-placeholder = Ce qui s'est passé, sur quelle cible
ops-journal-filter = Filtrer le journal
ops-journal-export-csv = Exporter en CSV
ops-journal-count =
    { $count ->
        [one] { $count } entrée
       *[other] { $count } entrées
    }

ops-hosts-column-address = Adresse
ops-hosts-column-name = Nom
ops-hosts-column-os = Système
ops-hosts-column-status = Statut
ops-hosts-column-ports = Ports
ops-hosts-import-nmap = Importer un scan nmap
ops-hosts-count =
    { $count ->
        [one] { $count } hôte
       *[other] { $count } hôtes
    }

ops-finding-new = Nouveau constat
ops-finding-severity = Sévérité
ops-finding-status = Statut
ops-finding-assets = Actifs touchés
ops-finding-cwe = CWE
ops-finding-cwe-placeholder = Ajouter une faiblesse : numéro ou nom
ops-finding-cwe-remove = Retirer cette faiblesse
ops-finding-cwe-filter = Filtrer par CWE
ops-finding-cwe-invalid = { $value } n'est ni un identifiant CWE ni un nom du catalogue.
ops-finding-cwe-none = Aucune faiblesse ne correspond
ops-finding-body = Texte
ops-finding-body-placeholder = Rédigez le constat ici (Markdown)
ops-finding-body-save = Enregistrer le texte
ops-finding-body-reload = Recharger depuis la note
ops-finding-preview = Aperçu
ops-finding-attack = Techniques ATT&CK
ops-finding-score = Score CVSS
ops-finding-vector = Vecteur CVSS
ops-finding-count =
    { $count ->
        [one] { $count } constat
       *[other] { $count } constats
    }

ops-cvss-calculator = Calculateur CVSS
ops-cvss-version = Version { $version }
ops-cvss-invalid-vector = Ce vecteur CVSS n'est pas valide.

ops-credentials-column-user = Utilisateur
ops-credentials-column-secret = Secret
ops-credentials-column-source = Source
ops-credentials-add = Ajouter un identifiant
ops-credentials-reveal = Afficher
ops-credentials-hide = Masquer

ops-checklist-progress = { $done } sur { $total } faits
ops-checklist-next-step = Prochaine étape
ops-checklist-from-note = Utiliser une note comme gabarit

ops-exam-profile = Profil d'examen
ops-exam-time-left = Il reste { $duration }
ops-exam-time-over = Le temps est écoulé
ops-exam-alert = Il reste moins de { $minutes } min
ops-exam-points =
    { $points ->
        [one] { $points } point
       *[other] { $points } points
    }
ops-exam-phase-practical = Phase pratique
ops-exam-phase-report = Phase de rapport
ops-exam-rules = Règles

ops-report-build = Assembler le rapport
ops-report-export-html = Exporter en HTML
ops-report-export-pdf = Exporter en PDF
ops-evidence-paste = Coller une capture comme preuve
ops-reminder-sent-data = Ce qui quitte votre machine

ops-stat-hosts = Hôtes
ops-stat-owned = Compromis
ops-stat-findings = Constats
ops-stat-critical = Critiques
ops-stat-checklist = Liste de contrôle
ops-latest-events = Derniers événements

ops-exam-oscp-rule-1 = L'examen est surveillé.
ops-exam-oscp-rule-2 = Fichiers de preuve local.txt et proof.txt : lus dans un shell interactif sur la cible avec type ou cat, depuis leur emplacement d'origine ; un web shell ne suffit pas. Chaque preuve figure dans une capture d'écran du rapport.
ops-exam-oscp-rule-3 = Sous Windows : un shell SYSTEM, Administrator ou de niveau administrateur ; sous Linux : un shell root.
ops-exam-oscp-rule-4 = Metasploit et Meterpreter : une seule machine cible au choix, fixée au premier usage.
ops-exam-oscp-rule-5 = Interdits : usurpation (IP, ARP, DNS, NBNS), outils commerciaux, exploitation automatique, scanners de vulnérabilités de masse, assistants IA et modèles de langage.
ops-exam-oscp-rule-6 = Rapport : chaque attaque documentée pas à pas, remis dans les 24 h après l'examen ; la remise est définitive.
ops-exam-osep-rule-1 = L'examen est surveillé.
ops-exam-osep-rule-2 = Fichiers de preuve local.txt et proof.txt : lus dans un shell interactif sur la cible ; un web shell ou une session RDP ne suffit pas.
ops-exam-osep-rule-3 = Rapport en PDF archivé dans un .7z, scripts et preuves de concept copiés en texte dans le PDF ; la remise est définitive.
ops-exam-osep-rule-4 = Une preuve absente de la capture donne zéro point pour la cible.
ops-exam-oswe-rule-1 = L'examen est surveillé.
ops-exam-oswe-rule-2 = Rapport en PDF archivé dans un .7z, script de preuve de concept collé en texte (ou en base64) dans le PDF ; la remise est définitive.
ops-exam-oswe-rule-3 = Une preuve ou une documentation manquante peut réduire les points d'un objectif, ou les annuler.
ops-exam-osed-rule-1 = L'examen est surveillé.
ops-exam-osed-rule-2 = Trois tâches ; des points ne sont attribués que pour une tâche entièrement terminée.
ops-exam-osed-rule-3 = Une preuve par cible, soumise dans le panneau de contrôle et montrée dans une capture d'écran.
ops-exam-osed-rule-4 = Rapport décrivant la découverte et l'exploitation de chaque tâche résolue ; la remise est définitive.
ops-exam-crtp-rule-1 = À la fin de l'examen : solutions détaillées des défis et mesures d'atténuation pratiques à remettre.
ops-exam-crtp-rule-2 = Une tentative incluse ; les pratiques déloyales sont sanctionnées par une pause de six mois.
ops-exam-crte-rule-1 = À la fin de l'examen : solutions détaillées des défis et mesures d'atténuation pratiques à remettre.
ops-exam-crte-rule-2 = Environnement à plusieurs domaines et forêts.
ops-exam-crto-rule-1 = 48 heures de laboratoire, utilisables dans une fenêtre de 4 jours.
ops-exam-crto-rule-2 = Un drapeau par machine, soumis sur le tableau des scores : 6 sur 8 pour réussir. Le barème est en drapeaux, pas en points.
ops-exam-pnpt-rule-1 = Du renseignement en sources ouvertes à la compromission du contrôleur de domaine de l'examen.
ops-exam-pnpt-rule-2 = Rapport écrit professionnel, puis un débriefing oral de 15 minutes devant les évaluateurs.
ops-exam-cpts-rule-1 = Rapport de niveau commercial à déposer sur la page de l'examen dans les 10 jours suivant l'entrée en examen.
ops-exam-cpts-rule-2 = Plusieurs drapeaux à soumettre sur la page de l'examen ; un nombre minimal de points est exigé avec le rapport.
ops-exam-cpts-rule-3 = Un rapport remis après l'échéance fait expirer le bon d'examen, sans seconde tentative.
ops-exam-bscp-rule-1 = Deux applications, trois étapes chacune : accéder à un compte, accéder à l'interface d'administration, lire /home/carlos/secret.
ops-exam-bscp-rule-2 = Un fichier de projet Burp doit être utilisé pendant tout l'examen et soumis pour analyse.
ops-exam-bscp-rule-3 = Aucune aide extérieure ; ne partagez pas les adresses de l'examen.
ops-exam-ejpt-rule-1 = Score minimal de 70 % au total et un minimum dans chaque domaine.
ops-exam-ejpt-rule-2 = Questions à choix multiples et laboratoires ; notation automatique.
ops-exam-ecppt-rule-1 = Score minimal de 70 % au total ; notation automatique.
ops-exam-ecppt-rule-2 = Bon valable 180 jours, une reprise gratuite dans les 14 jours.
ops-exam-oscp-machine-1 = Machine isolée 1
ops-exam-oscp-machine-2 = Machine isolée 2
ops-exam-oscp-machine-3 = Machine isolée 3
ops-exam-oscp-machine-4 = Ensemble Active Directory, machine 1
ops-exam-oscp-machine-5 = Ensemble Active Directory, machine 2
ops-exam-oscp-machine-6 = Ensemble Active Directory, machine 3
ops-exam-milestone-initial-access = Accès initial (local.txt)
ops-exam-milestone-privilege-escalation = Élévation de privilèges (proof.txt)

ops-cvss-metric-av = Vecteur d'attaque
ops-cvss-metric-ac = Complexité de l'attaque
ops-cvss-metric-at = Exigences de l'attaque
ops-cvss-metric-pr = Privilèges requis
ops-cvss-metric-ui = Interaction de l'utilisateur
ops-cvss-metric-s = Portée
ops-cvss-metric-c = Confidentialité
ops-cvss-metric-i = Intégrité
ops-cvss-metric-a = Disponibilité
ops-cvss-metric-vc = Confidentialité du système vulnérable
ops-cvss-metric-vi = Intégrité du système vulnérable
ops-cvss-metric-va = Disponibilité du système vulnérable
ops-cvss-metric-sc = Confidentialité du système subséquent
ops-cvss-metric-si = Intégrité du système subséquent
ops-cvss-metric-sa = Disponibilité du système subséquent
ops-cvss-value-av-n = Réseau
ops-cvss-value-av-a = Adjacent
ops-cvss-value-av-l = Local
ops-cvss-value-av-p = Physique
ops-cvss-value-ac-l = Faible
ops-cvss-value-ac-h = Élevée
ops-cvss-value-at-n = Aucune
ops-cvss-value-at-p = Présentes
ops-cvss-value-pr-n = Aucun
ops-cvss-value-pr-l = Faibles
ops-cvss-value-pr-h = Élevés
ops-cvss-value-ui-n = Aucune
ops-cvss-value-ui-r = Requise
ops-cvss-value-ui-p = Passive
ops-cvss-value-ui-a = Active
ops-cvss-value-s-u = Inchangée
ops-cvss-value-s-c = Modifiée
ops-cvss-value-c-h = Haute
ops-cvss-value-c-l = Faible
ops-cvss-value-c-n = Aucune
ops-cvss-value-i-h = Haute
ops-cvss-value-i-l = Faible
ops-cvss-value-i-n = Aucune
ops-cvss-value-a-h = Haute
ops-cvss-value-a-l = Faible
ops-cvss-value-a-n = Aucune
ops-cvss-value-vc-h = Haute
ops-cvss-value-vc-l = Faible
ops-cvss-value-vc-n = Aucune
ops-cvss-value-vi-h = Haute
ops-cvss-value-vi-l = Faible
ops-cvss-value-vi-n = Aucune
ops-cvss-value-va-h = Haute
ops-cvss-value-va-l = Faible
ops-cvss-value-va-n = Aucune
ops-cvss-value-sc-h = Haute
ops-cvss-value-sc-l = Faible
ops-cvss-value-sc-n = Aucune
ops-cvss-value-si-h = Haute
ops-cvss-value-si-l = Faible
ops-cvss-value-si-n = Aucune
ops-cvss-value-sa-h = Haute
ops-cvss-value-sa-l = Faible
ops-cvss-value-sa-n = Aucune

ops-var-rhost-desc = Adresse de la cible
ops-var-lhost-desc = Adresse de la machine locale
ops-var-lport-desc = Port d'écoute local
ops-var-domain-desc = Domaine
ops-var-user-desc = Nom d'utilisateur
ops-var-pass-desc = Mot de passe
ops-var-hash-desc = Empreinte du mot de passe
ops-var-dc-desc = Contrôleur de domaine
ops-var-dc_host-desc = Nom d'hôte du contrôleur de domaine
ops-var-domain_sid-desc = SID du domaine
ops-var-url-desc = Adresse web de la cible
ops-var-wordlist-desc = Fichier de liste de mots
ops-var-hostname-desc = Nom de la cible
ops-var-interface-desc = Interface réseau

ops-checklist-mission-label = Déroulement de la mission
ops-checklist-exam-label = Déroulement de l'examen

ops-cancel = Annuler
ops-close = Fermer
ops-commands-count = Commandes : { $count }
ops-commands-empty = Aucune commande trouvée. Focus lit les blocs de code shell de vos propres notes, dans les dossiers choisis dans les réglages.
ops-commands-journaled = Commande ajoutée au journal.
ops-commands-no-match = Aucune commande ne correspond à cette recherche
ops-commands-search = Rechercher par outil, commande ou titre
ops-commands-to-journal = Ajouter au journal
ops-copied = Copié
ops-credentials-column-domain = Domaine
ops-credentials-column-kind = Type
ops-credentials-copy = Copier le secret
ops-credentials-count =
    { $count ->
        [one] { $count } identifiant
       *[other] { $count } identifiants
    }
ops-credentials-duplicate = Cet identifiant est déjà enregistré.
ops-credentials-empty = Aucun identifiant pour l'instant. Ajoutez-en un ci-dessus.
ops-cvss-apply = Appliquer au constat sélectionné
ops-exam-points-label = Points
ops-exam-points-of = { $points } sur { $pass } pour réussir ({ $total } au total)
ops-exam-points-pass = { $points } sur { $pass } pour réussir
ops-filter-all = Tous
ops-finding-create = Créer
ops-finding-title-placeholder = Titre du constat
ops-finding-vector-copy = Copier le vecteur
ops-findings-empty = Aucun constat à afficher. Créez-en un avec Nouveau constat.
ops-hosts-empty = Aucun hôte pour l'instant. Importez un scan nmap pour remplir ce tableau.
ops-hosts-import = Importer
ops-hosts-import-hint = Collez la sortie d'un scan nmap (XML, grepable ou normale).
ops-hosts-import-report = Créés : { $created }, mis à jour : { $updated }, services lus : { $services }.
ops-hosts-paste-placeholder = Collez le scan ici
ops-journal-all-targets = Toutes les cibles
ops-journal-empty = Le journal est vide. Ajoutez une entrée depuis la barre Ops.
ops-journal-no-match = Aucune entrée ne correspond à ce filtre
ops-loading = Lecture de l'engagement…
ops-new-client = Client
ops-new-create = Créer l'engagement
ops-new-kind = Type d'engagement
ops-new-name = Nom
ops-new-name-required = Donnez un nom à l'engagement.
ops-new-scope-placeholder = Adresses ou domaines
ops-new-template = Gabarit de liste de contrôle
ops-new-template-default = Celui du type d'engagement
ops-no-target = Aucune cible
ops-open-note = Ouvrir la note
ops-exam-rules-count = Règles : { $count }

ops-credentials-column-valid = Valable sur
ops-host-cited-none = Rien ne cite encore cet hôte.
ops-host-cited-findings = Constats qui citent cet hôte
ops-host-cited-credentials = Identifiants qui citent cet hôte
ops-journal-copy-link = Copier le lien de cette entrée
ops-report-author = Auteur
ops-report-empty = Construisez le rapport pour le voir ici.
ops-report-include-credentials = Identifiants
ops-report-include-journal = Journal
ops-report-mask-secrets = Masquer les secrets
ops-report-preview = Aperçu
ops-report-secrets-warning = Le rapport contiendra des secrets en clair.
ops-report-write = Écrire Report.md
ops-report-written = { $file } écrit.
ops-new-note-search = Chercher une note avec des cases à cocher
ops-var-add = Ajouter la variable
ops-var-invalid-name = Un nom de variable utilise des lettres, des chiffres et des tirets bas, et ne commence pas par un chiffre.
ops-var-name-placeholder = NOM
ops-var-value-placeholder = Valeur

ops-finding-sheet = Constat
ops-finding-sheet-empty = Sélectionnez un constat pour lire sa note.

ops-exam-start = Démarrer le chronomètre de l'examen
ops-exam-not-started = { $minutes } minutes, pas démarré
ops-finding-edit = Modifier
ops-finding-edit-done = Terminé
ops-finding-techniques = Techniques ATT&CK
ops-finding-asset-placeholder = Ajouter un actif (hôte ou adresse)
ops-finding-technique-placeholder = Ajouter une technique (T1059.001)
ops-finding-asset-remove = Retirer cet actif
ops-finding-technique-remove = Retirer cette technique
ops-finding-technique-invalid = « { $value } » n'est pas un identifiant de technique ATT&CK.
ops-filter-severity = Gravité : { $value }
ops-filter-status = Statut : { $value }
ops-scope = Périmètre
ops-scope-placeholder = Adresses ou domaines, Entrée pour enregistrer
ops-hosts-import-file = Choisir un fichier de scan…
ops-sort-severity = Trié par gravité
ops-sort-creation = Plus récent d'abord

ops-targets-address = Adresse, plage ou nom
ops-targets-hostname = Nom d'hôte
ops-targets-os = Système
ops-targets-tags = Tags, séparés par des virgules
ops-targets-paste-placeholder = 10.0.0.5
    10.0.1.0/24
    dc01.corp.local
ops-targets-add-button = Ajouter des cibles
ops-targets-manual = Ajouter une cible
ops-targets-add = Ajouter
ops-targets-list = Coller une liste ou lire un fichier
ops-targets-hint = Une cible par ligne : une adresse, une plage (CIDR) ou un nom, puis éventuellement un nom d'hôte. # commence un commentaire. Un CSV avec une ligne d'en-tête est lu d'après ses en-têtes : ip ou address, hostname ou name, os, tags, notes (virgule, point-virgule ou tabulation).
ops-targets-preview-button = Aperçu
ops-targets-file = Choisir un fichier texte ou CSV…
ops-targets-known = Cette cible est déjà dans l'engagement.
ops-targets-created = Cibles créées : { $count }.
ops-targets-preview = À créer : { $create }. Déjà présentes : { $known }. Refusées : { $refused }.
ops-targets-more = et { $count } de plus
ops-targets-known-title = Déjà présentes, laissées telles quelles
ops-targets-refused-title = Refusées
ops-targets-refused-line = Ligne { $line } : { $text } ({ $reason })
ops-targets-confirm = Créer les cibles ({ $count })
ops-targets-reason-address = ni une adresse, ni une plage, ni un nom
ops-targets-reason-name = pas un nom d'hôte
ops-targets-reason-extra = texte après l'adresse et le nom
ops-targets-reason-long = trop long
ops-targets-reason-repeated = listée deux fois
ops-targets-reason-many = trop de cibles dans une liste
ops-targets-reason-none = pas d'adresse
