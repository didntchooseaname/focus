# Onglets

````tabs

tab: Linux

Texte Linux premier groupe.

tab: Windows

### Titre dans Windows

Texte Windows premier groupe.

````

Entre deux groupes.

````tabs

tab: Linux

Autre groupe, titre Linux.

tab: Windows

Autre groupe, même titre Windows.

````

{{#tabs}}
{{#tab name="Un"}}
Texte un.
{{#tabs}}
{{#tab name="Interne"}}
Texte interne.
{{#endtab}}
{{#endtabs}}
{{#endtab}}
{{#tab name="Deux"}}
## Section dans Deux

Texte deux avec mot-rare-cache.
{{#endtab}}
{{#endtabs}}

{{#ref}}
Cible.md
{{#endref}}
