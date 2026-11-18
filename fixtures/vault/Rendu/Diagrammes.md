---
type: reference
tags: [focus/demo, mermaid]
---

# Diagrammes

## Flowchart

```mermaid
flowchart LR
    A[Reconnaissance] --> B{Compte valide ?}
    B -- oui --> C[Kerberoasting]
    B -- non --> D[AS-REP roasting]
    C --> E[Cassage hors ligne]
    D --> E
    E --> F[(Identifiants)]
    F --> G[Mouvement latéral]
```

## Séquence

```mermaid
sequenceDiagram
    participant C as Client
    participant K as KDC
    participant S as Service
    C->>K: AS-REQ (pré-authentification)
    K-->>C: AS-REP (TGT)
    C->>K: TGS-REQ (SPN visé)
    K-->>C: TGS-REP (ticket chiffré avec la clé du service)
    C->>S: AP-REQ
    Note over C,S: Le ticket se casse hors ligne
```

## États

```mermaid
stateDiagram-v2
    [*] --> Nouveau
    Nouveau --> Enumeration
    Enumeration --> Acces
    Acces --> Utilisateur
    Utilisateur --> Compromis
    Compromis --> [*]
```

## Classes

```mermaid
classDiagram
    class Engagement {
        +String nom
        +Date debut
        +creerConstat()
    }
    class Hote {
        +String ip
        +Service[] services
    }
    Engagement "1" --> "*" Hote
```

## Entité-relation

```mermaid
erDiagram
    ENGAGEMENT ||--o{ HOTE : contient
    HOTE ||--o{ SERVICE : expose
    ENGAGEMENT ||--o{ CONSTAT : produit
```

## Gantt

```mermaid
gantt
    title Test interne
    dateFormat YYYY-MM-DD
    section Préparation
    Cadrage           :done, a1, 2026-11-02, 2d
    section Exécution
    Énumération       :active, a2, after a1, 3d
    Exploitation      :a3, after a2, 4d
    section Restitution
    Rapport           :a4, after a3, 3d
```

## Camembert

```mermaid
pie title Constats par sévérité
    "Critique" : 2
    "Haute" : 5
    "Moyenne" : 9
    "Basse" : 4
```

## Carte mentale

```mermaid
mindmap
  root((Active Directory))
    Kerberos
      Kerberoasting
      AS-REP roasting
      Délégation
    ADCS
      ESC1
      ESC8
    Relais NTLM
```

## Chronologie

```mermaid
timeline
    title Journée d'examen
    09h00 : Début
    11h30 : Premier accès
    15h00 : Jeu Active Directory compromis
    23h45 : Fin de l'épreuve
```

## Erreur de syntaxe

```mermaid
flowchart LR
    A --> 
    --> B[
```
