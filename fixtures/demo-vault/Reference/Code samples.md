---
type: reference
status: ready
tags: [reference, evidence]
last_verified: 2026-11-12
---

# Code samples

Fenced blocks with a language. Inventory context: [[Asset inventory]], [[Service inventory]], [[Screenshot conventions]].

## Bash

```bash
TARGET="192.0.2.20"
DOMAIN="northwind.example.com"
nmap -sV "$TARGET"
curl -I "https://$DOMAIN"
```

## JSON

```json
{
  "client": "Northwind Labs",
  "domain": "northwind.example.com",
  "in_scope": ["192.0.2.0/24"],
  "retest": true
}
```

## YAML

```yaml
engagement:
  name: Northwind internal
  start: 2026-11-09
  contacts:
    - role: technical
      mail: tech@northwind.example.com
```

## Rust

```rust
fn severity(score: f32) -> &'static str {
    match score {
        s if s >= 9.0 => "critical",
        s if s >= 7.0 => "high",
        s if s >= 4.0 => "medium",
        _ => "low",
    }
}
```

## Python

```python
def hours(entries):
    return sum(e["minutes"] for e in entries) / 60
```

Inline: run `date -I` to stamp a note.
