search-placeholder = Rechercher dans le coffre
search-searching = Recherche en cours…
search-no-results = Aucun résultat.
search-results-count =
    { $count ->
        [one] { $count } résultat dans { $files ->
            [one] { $files } note
           *[other] { $files } notes
        }
       *[other] { $count } résultats dans { $files ->
            [one] { $files } note
           *[other] { $files } notes
        }
    }
search-results-truncated = Résultats coupés à { $limit } notes. Affinez la requête.
search-elapsed = { $duration }
search-option-case = Respecter la casse
search-option-word = Mot entier
search-option-regex = Expression régulière
search-sort-relevance = Pertinence
search-sort-path = Chemin
search-sort-newest = Plus récentes d'abord
search-sort-oldest = Plus anciennes d'abord
search-ai-toggle = IA
search-ai-queries = Requêtes générées par le modèle
search-ai-answer = Réponse
search-ai-citation = Source { $number }
search-op-tag = Notes portant ce tag
search-op-path = Notes dont le chemin contient le texte
search-op-file = Notes dont le nom de fichier contient le texte
search-op-title = Notes dont le titre contient le texte
search-op-heading = Notes ayant un titre de section qui contient le texte
search-op-task = Notes ayant une tâche qui contient le texte
search-op-prop = Notes dont une propriété du frontmatter correspond
search-op-exclude = Exclut le terme
search-op-phrase = Phrase exacte
search-op-or = L'un ou l'autre terme
search-op-regex = Expression régulière entre barres obliques
search-tags-empty = Aucun tag dans ce coffre.
search-tag-count =
    { $count ->
        [one] { $count } note
       *[other] { $count } notes
    }
search-index-building = Construction de l'index : { $done } notes sur { $total }
search-index-ready = Index prêt : { $count } notes
search-empty-title = Rechercher dans vos notes
search-empty-hint = Saisissez des mots, une phrase entre guillemets ou un opérateur comme tag: ou path:.
search-none-hint = Essayez moins de mots, ou désactivez la casse et le mot entier.
search-clear = Effacer la recherche
search-help-toggle = Syntaxe de recherche
search-help-insert = Insérer cet exemple
search-sort = Trier les résultats
search-collapse = Replier la note
search-expand = Déplier la note
search-hint-open = Ouvrir
search-hint-new-tab = Nouvel onglet
search-hint-clear = Effacer
search-hint-ask = Interroger l’assistant
search-diag-unclosed-quote = Guillemet non fermé
search-diag-invalid-regex = Expression régulière invalide : { $detail }
search-diag-unknown-operator = Opérateur inconnu { $name } : cherché comme texte
search-diag-missing-value = { $operator } attend une valeur
search-diag-unclosed-parenthesis = Parenthèse non fermée
search-diag-unmatched-parenthesis = Parenthèse fermante sans ouvrante
search-ai-toggle-tip = Recherche assistée
search-ai-intent = Compris comme
search-ai-stop = Arrêter
search-ai-stage-expanding = Rédaction des requêtes…
search-ai-stage-searching = Recherche dans le coffre…
search-ai-stage-ranking = Classement des notes…
search-ai-stage-answering = Rédaction de la réponse…
search-ai-stopped = Arrêtée
search-ai-nothing = Aucune note ne correspond aux requêtes générées.
search-ai-cost = Environ { $amount } USD
search-ai-query-add = Ajouter une requête
search-ai-query-edit = Modifier cette requête
search-ai-query-remove = Retirer cette requête
search-ai-query-placeholder = Nouvelle requête
search-ai-failed-missing-key = Aucune clé d’API. Ajoutez-en une dans Réglages › IA.
search-ai-failed-invalid-key = DeepSeek a refusé la clé d’API (401). Vérifiez-la dans Réglages › IA.
search-ai-failed-balance = Le solde DeepSeek est insuffisant (402). Rechargez le compte.
search-ai-failed-rate-limit = Limite de débit DeepSeek atteinte. Réessayez dans { $seconds } s.
search-ai-failed-network = DeepSeek est injoignable. Les résultats locaux sont conservés.
search-ai-failed-other = L’assistant a échoué. Les résultats locaux sont conservés.
search-replace-toggle = Remplacer dans tout le coffre
search-replace-placeholder = Remplacer par
search-replace-reading = Lecture des notes…
search-replace-summary =
    { $lines ->
        [one] { $lines } ligne à modifier dans { $files ->
            [one] { $files } note
           *[other] { $files } notes
        }
       *[other] { $lines } lignes à modifier dans { $files ->
            [one] { $files } note
           *[other] { $files } notes
        }
    }
search-replace-kept = { $lines } retenues dans { $files }
search-replace-apply = Appliquer
search-replace-no-terms = La requête ne contient aucun texte à remplacer.
search-replace-done =
    { $count ->
        [one] { $count } occurrence remplacée dans { $files ->
            [one] { $files } note
           *[other] { $files } notes
        }
       *[other] { $count } occurrences remplacées dans { $files ->
            [one] { $files } note
           *[other] { $files } notes
        }
    }
search-replace-failed =
    { $count ->
        [one] { $count } note n’a pas été écrite
       *[other] { $count } notes n’ont pas été écrites
    }
search-replace-changed = modifiée sur le disque depuis l’aperçu
search-replace-write-error = écriture impossible ({ $detail })
search-replace-unreadable =
    { $count ->
        [one] { $count } note écartée : UTF-8 invalide
       *[other] { $count } notes écartées : UTF-8 invalide
    }
