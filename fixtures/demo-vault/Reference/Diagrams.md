---
type: reference
status: ready
tags: [reference, method]
last_verified: 2026-11-12
---

# Diagrams

Ten Mermaid diagrams about one engagement. Method notes: [[Engagement lifecycle]], [[Scoping call checklist]], [[Time tracking]].

## Flowchart

```mermaid
flowchart TD
    A[Scoping call] --> B{Scope signed?}
    B -- yes --> C[Kickoff]
    B -- no --> A
    C --> D[Testing]
    D --> E[Report]
    E --> F[Retest]
```

## Sequence

```mermaid
sequenceDiagram
    participant T as Tester
    participant C as Client
    T->>C: Request access list
    C-->>T: Send accounts
    T->>C: Weekly status mail
    C-->>T: Questions
```

## State

```mermaid
stateDiagram-v2
    [*] --> Draft
    Draft --> Review
    Review --> Ready
    Review --> Draft
    Ready --> [*]
```

## Class

```mermaid
classDiagram
    class Engagement {
        +String client
        +Date start
        +close()
    }
    class Finding {
        +String title
        +Float score
    }
    Engagement "1" --> "*" Finding
```

## Entity relationship

```mermaid
erDiagram
    CLIENT ||--o{ ENGAGEMENT : orders
    ENGAGEMENT ||--|{ FINDING : contains
    ENGAGEMENT ||--o{ ASSET : covers
```

## Gantt

```mermaid
gantt
    title Engagement plan
    dateFormat YYYY-MM-DD
    section Preparation
    Scoping     :a1, 2026-11-02, 3d
    Kickoff     :after a1, 1d
    section Testing
    Fieldwork   :2026-11-09, 8d
    Report      :2026-11-19, 4d
```

## Pie

```mermaid
pie title Findings by severity
    "Critical" : 1
    "High" : 3
    "Medium" : 6
    "Low" : 9
```

## Mindmap

```mermaid
mindmap
  root((Engagement))
    Method
      Scoping
      Journal
    Reporting
      Findings
      Summary
    Delivery
      Handover
      Retest
```

## Timeline

```mermaid
timeline
    title Engagement milestones
    2026-11-02 : Scoping call
    2026-11-09 : Kickoff
    2026-11-19 : Draft report
    2026-12-03 : Handover
```

## Second flowchart

```mermaid
flowchart LR
    N[Note] --> E[Evidence]
    E --> F[Finding]
    F --> R[Report]
```
