stateDiagram-v2
    state "مفتوحة" as Open
    state "مغلقة" as Closed
    [*] --> Open
    Open --> Closed
    Closed --> [*]
