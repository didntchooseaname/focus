use crate::vars;
use crate::{Credential, Engagement};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    User,
    Secret,
    Domain,
}

pub fn field_of(canonical: &str) -> Option<Field> {
    match canonical {
        "USER" => Some(Field::User),
        "PASS" | "HASH" => Some(Field::Secret),
        "DOMAIN" => Some(Field::Domain),
        _ => None,
    }
}

pub fn is_secret(canonical: &str) -> bool {
    matches!(canonical, "PASS" | "HASH")
}

fn kind_fits(canonical: &str, kind: &str) -> bool {
    match canonical {
        "PASS" => kind.eq_ignore_ascii_case("password"),
        "HASH" => ["ntlm", "hash"]
            .iter()
            .any(|k| kind.eq_ignore_ascii_case(k)),
        _ => true,
    }
}

fn key_of(credential: &Credential) -> String {
    serde_json::to_string(&[&credential.user, &credential.domain, &credential.kind])
        .unwrap_or_default()
}

fn value_of(canonical: &str, credential: &Credential) -> Option<String> {
    let value = match field_of(canonical)? {
        Field::User => &credential.user,
        Field::Secret => &credential.secret,
        Field::Domain => &credential.domain,
    };
    Some(value.clone()).filter(|v| !v.is_empty())
}

#[derive(Debug, Clone)]
pub struct Choice {
    pub credential: Credential,
    pub label: String,
    pub value: String,
}

pub fn choices(credentials: &[Credential], name: &str) -> Vec<Choice> {
    let canonical = vars::canonical_name(name);
    let mut out: Vec<Choice> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for credential in credentials
        .iter()
        .filter(|c| kind_fits(&canonical, &c.kind))
    {
        let Some(value) = value_of(&canonical, credential) else {
            continue;
        };
        let who = if credential.domain.is_empty() {
            credential.user.clone()
        } else {
            format!("{}\\{}", credential.domain, credential.user)
        };
        let (label, once) = match field_of(&canonical) {
            Some(Field::User) => (who, format!("{}\0{}", credential.user, credential.domain)),
            Some(Field::Domain) => (value.clone(), value.clone()),
            _ => (
                format!("{who} ({})", credential.kind),
                format!("{}\0{value}", key_of(credential)),
            ),
        };
        if !seen.contains(&once) {
            seen.push(once);
            out.push(Choice {
                credential: credential.clone(),
                label,
                value,
            });
        }
    }
    out
}

fn find<'a>(credentials: &'a [Credential], key: &str, canonical: &str) -> Option<&'a Credential> {
    let exact = credentials.iter().rev().find(|c| key_of(c) == key);
    exact.or_else(|| {
        let [user, domain, _]: [String; 3] = serde_json::from_str(key).ok()?;
        matches!(field_of(canonical), Some(Field::User | Field::Domain))
            .then(|| {
                credentials
                    .iter()
                    .rev()
                    .find(|c| c.user == user && c.domain == domain)
            })
            .flatten()
    })
}

pub fn link(engagement: &mut Engagement, name: &str, credential: &Credential) -> bool {
    let canonical = vars::canonical_name(name);
    let Some(value) = kind_fits(&canonical, &credential.kind)
        .then(|| value_of(&canonical, credential))
        .flatten()
    else {
        return false;
    };
    vars::store(&mut engagement.vars, &canonical, &value);
    engagement.var_links.insert(canonical, key_of(credential));
    true
}

pub fn unlink(engagement: &mut Engagement, name: &str) -> bool {
    engagement
        .var_links
        .remove(&vars::canonical_name(name))
        .is_some()
}

pub fn use_credential(engagement: &mut Engagement, credential: &Credential) -> Vec<String> {
    ["USER", "DOMAIN", "PASS", "HASH"]
        .into_iter()
        .filter(|name| link(engagement, name, credential))
        .map(str::to_string)
        .collect()
}

pub fn follow(engagement: &mut Engagement, credentials: &[Credential]) {
    let links: Vec<(String, String)> = engagement
        .var_links
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (name, key) in links {
        match find(credentials, &key, &name) {
            Some(credential) => match value_of(&name, credential) {
                Some(value) => {
                    vars::store(&mut engagement.vars, &name, &value);
                    engagement.var_links.insert(name, key_of(credential));
                }
                None => {
                    engagement.var_links.remove(&name);
                }
            },
            None => {
                engagement.var_links.remove(&name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EngagementKind, EngagementStatus};

    fn credential(user: &str, kind: &str, secret: &str, domain: &str) -> Credential {
        Credential {
            user: user.into(),
            kind: kind.into(),
            secret: secret.into(),
            domain: domain.into(),
            source: String::new(),
            valid_on: String::new(),
            notes: String::new(),
        }
    }

    fn engagement() -> Engagement {
        Engagement {
            dir: Default::default(),
            name: "x".into(),
            kind: EngagementKind::Lab,
            status: EngagementStatus::InProgress,
            client: None,
            started: None,
            deadline: None,
            scope: vec![],
            out_of_scope: vec![],
            vars: Default::default(),
            var_links: Default::default(),
            pauses: vec![],
            stopped: None,
            exam: None,
            exam_done: vec![],
            focus: None,
        }
    }

    fn table() -> Vec<Credential> {
        vec![
            credential("alice", "password", "Winter2026!", "corp.local"),
            credential(
                "alice",
                "ntlm",
                "aad3b435b51404eeaad3b435b51404ee",
                "corp.local",
            ),
            credential("bob", "password", "hunter2", ""),
            credential("svc", "ssh-key", "KEY", "corp.local"),
        ]
    }

    #[test]
    fn each_variable_is_offered_what_suits_it_without_any_secret_in_the_label() {
        let credentials = table();
        let labels = |name: &str| {
            choices(&credentials, name)
                .into_iter()
                .map(|c| c.label)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            labels("USER"),
            ["corp.local\\alice", "bob", "corp.local\\svc"]
        );
        assert_eq!(
            labels("password"),
            ["corp.local\\alice (password)", "bob (password)"],
            "an alias asks for PASS"
        );
        assert_eq!(labels("HASH"), ["corp.local\\alice (ntlm)"]);
        assert_eq!(labels("domain"), ["corp.local"]);
        assert!(labels("RHOST").is_empty());
        for choice in choices(&credentials, "PASS")
            .iter()
            .chain(&choices(&credentials, "HASH"))
        {
            assert!(!choice.label.contains(&choice.value), "{}", choice.label);
        }
    }

    #[test]
    fn using_a_credential_sets_user_domain_and_the_secret_of_its_type() {
        let credentials = table();
        let mut e = engagement();
        assert_eq!(
            use_credential(&mut e, &credentials[0]),
            ["USER", "DOMAIN", "PASS"]
        );
        assert_eq!(
            (
                e.vars["USER"].as_str(),
                e.vars["DOMAIN"].as_str(),
                e.vars["PASS"].as_str()
            ),
            ("alice", "corp.local", "Winter2026!")
        );
        assert_eq!(
            use_credential(&mut e, &credentials[1]),
            ["USER", "DOMAIN", "HASH"]
        );
        assert_eq!(
            use_credential(&mut e, &credentials[2]),
            ["USER", "PASS"],
            "no domain, no DOMAIN"
        );
        assert_eq!(
            e.vars["DOMAIN"], "corp.local",
            "a variable the credential cannot give keeps its value"
        );
        assert!(
            !link(&mut e, "HASH", &credentials[0]),
            "a password is not a hash"
        );
    }

    #[test]
    fn a_link_names_the_credential_and_never_its_secret() {
        let mut e = engagement();
        link(&mut e, "PASS", &table()[0]);
        assert_eq!(e.var_links["PASS"], r#"["alice","corp.local","password"]"#);
        assert!(!e.var_links["PASS"].contains("Winter2026!"));
    }

    #[test]
    fn a_linked_variable_follows_its_credential_and_survives_its_removal() {
        let mut e = engagement();
        link(&mut e, "PASS", &table()[0]);
        link(&mut e, "USER", &table()[0]);
        let mut changed = table();
        changed.push(credential("alice", "password", "Spring2027!", "corp.local"));
        follow(&mut e, &changed);
        assert_eq!(
            e.vars["PASS"], "Spring2027!",
            "the newest row of that user, domain and type"
        );
        let mut renamed_secret_gone = table();
        renamed_secret_gone.retain(|c| c.user != "alice");
        follow(&mut e, &renamed_secret_gone);
        assert_eq!(
            (e.vars["PASS"].as_str(), e.vars["USER"].as_str()),
            ("Spring2027!", "alice"),
            "the last value stays"
        );
        assert!(e.var_links.is_empty(), "the links are dropped");
    }

    #[test]
    fn links_are_kept_in_the_engagement_and_followed_when_it_is_loaded() {
        let ops = crate::fsutil::test_dir("links-load");
        let created = crate::create(
            &ops,
            &crate::NewEngagement {
                name: "Links".into(),
                kind: EngagementKind::Lab,
                client: None,
                scope: vec![],
                exam: None,
                checklist: None,
            },
        )
        .unwrap();
        crate::loot::add(
            &created.dir,
            &credential("alice", "password", "Winter2026!", "corp.local"),
        )
        .unwrap();
        let mut e = crate::load(&created.dir).unwrap();
        use_credential(&mut e, &crate::loot::list(&created.dir)[0]);
        crate::save(&e).unwrap();
        let readme = std::fs::read_to_string(created.dir.join("README.md")).unwrap();
        assert!(
            readme.contains("var_links:") && readme.contains("USER"),
            "{readme}"
        );
        crate::loot::add(
            &created.dir,
            &credential("alice", "password", "Spring2027!", "corp.local"),
        )
        .unwrap();
        let again = crate::load(&created.dir).unwrap();
        assert_eq!(
            (again.vars["USER"].as_str(), again.vars["PASS"].as_str()),
            ("alice", "Spring2027!")
        );
        assert!(
            vars::context_for(&again, None).secret.contains("PASS")
                && !vars::context_for(&again, None).secret.contains("USER")
        );
    }

    #[test]
    fn a_user_link_survives_a_change_of_type() {
        let mut e = engagement();
        link(&mut e, "USER", &table()[0]);
        let only_hash: Vec<Credential> = table().into_iter().filter(|c| c.kind == "ntlm").collect();
        follow(&mut e, &only_hash);
        assert!(e.var_links.contains_key("USER"));
        unlink(&mut e, "user");
        assert!(e.var_links.is_empty());
    }
}
