use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct Context {
    pub values: BTreeMap<String, String>,
    pub secret: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placeholder {
    pub range: Range<usize>,
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct Substituted {
    pub text: String,
    pub resolved: Vec<(Range<usize>, String)>,
    pub written: Vec<String>,
    pub missing: Vec<(Range<usize>, String)>,
}

const BUILTIN: &[(&str, &[&str], &str)] = &[
    (
        "RHOST",
        &["IP", "RHOSTS", "TARGET", "TARGET_IP", "VICTIM"],
        "Target address",
    ),
    (
        "LHOST",
        &["ATTACKER", "KALI", "ATTACKER_IP"],
        "Local machine address",
    ),
    ("LPORT", &["PORT"], "Local listening port"),
    ("DOMAIN", &["DOM"], "Domain"),
    ("USER", &["USERNAME"], "User name"),
    ("PASS", &["PASSWORD", "PWD"], "Password"),
    ("HASH", &["NTLM", "NTHASH", "NT_HASH"], "Password hash"),
    ("DC", &["DC_IP"], "Domain controller"),
    ("DC_HOST", &[], "Domain controller host name"),
    ("DOMAIN_SID", &[], "Domain SID"),
    ("URL", &[], "Target web address"),
    ("WORDLIST", &[], "Word list file"),
    ("HOSTNAME", &["HOST"], "Target name"),
    ("INTERFACE", &["IFACE"], "Network interface"),
];

const SHELL_VARIABLES: &[&str] = &[
    "PATH",
    "HOME",
    "SHELL",
    "PWD",
    "OLDPWD",
    "UID",
    "EUID",
    "PPID",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "EDITOR",
    "VISUAL",
    "PAGER",
    "LOGNAME",
    "DISPLAY",
    "TMPDIR",
    "IFS",
    "PS1",
    "PS2",
    "BASH",
    "BASH_VERSION",
    "RANDOM",
    "SECONDS",
    "LINENO",
    "OSTYPE",
    "HOSTTYPE",
    "MACHTYPE",
    "SHLVL",
    "PIPESTATUS",
    "FUNCNAME",
    "BASH_SOURCE",
    "REPLY",
    "OPTARG",
    "OPTIND",
    "XDG_RUNTIME_DIR",
    "XDG_CONFIG_HOME",
    "SSH_AUTH_SOCK",
    "COLUMNS",
    "LINES",
    "MAIL",
    "TZ",
];

pub fn canonical_name(name: &str) -> String {
    let normalized = normalize(name);
    alias_target(&normalized).map_or(normalized, str::to_string)
}

fn normalize(name: &str) -> String {
    name.to_ascii_uppercase().replace('-', "_")
}

fn alias_target(normalized: &str) -> Option<&'static str> {
    BUILTIN
        .iter()
        .find(|(canonical, aliases, _)| *canonical == normalized || aliases.contains(&normalized))
        .map(|(canonical, _, _)| *canonical)
}

pub fn stored<'a>(
    vars: &'a BTreeMap<String, String>,
    name: &str,
) -> Option<(&'a String, &'a String)> {
    let wanted = canonical_name(name);
    vars.iter().find(|(key, _)| canonical_name(key) == wanted)
}

pub fn store(vars: &mut BTreeMap<String, String>, name: &str, value: &str) {
    let wanted = canonical_name(name);
    let spelling = stored(vars, name).map(|(key, _)| key.clone());
    vars.retain(|key, _| canonical_name(key) != wanted);
    if !value.is_empty() {
        vars.insert(
            spelling.unwrap_or_else(|| name.to_string()),
            value.to_string(),
        );
    }
}

fn canonical(name: &str, extra: &dyn Fn(&str) -> bool) -> Option<String> {
    let normalized = normalize(name);
    alias_target(&normalized)
        .map(str::to_string)
        .or_else(|| extra(&normalized).then_some(normalized))
}

static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[\s;&|(])(?:(?:export|local|readonly|declare(?:\s+-\w+)?|typeset)\s+)?([A-Za-z_][A-Za-z0-9_]*)=").expect("motif d'affectation")
});
static LOOP_VARIABLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\bfor\s+([A-Za-z_][A-Za-z0-9_]*)\s+in\b|\bread\s+(?:-\w+\s+)*([A-Za-z_][A-Za-z0-9_]*)",
    )
    .expect("motif de boucle")
});

fn assigned_names(text: &str) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = ASSIGNMENT
        .captures_iter(text)
        .filter_map(|c| c.get(1))
        .map(|m| normalize(m.as_str()))
        .collect();
    for captures in LOOP_VARIABLE.captures_iter(text) {
        if let Some(m) = captures.get(1).or_else(|| captures.get(2)) {
            names.insert(normalize(m.as_str()));
        }
    }
    names
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_dash_name_byte(b: u8) -> bool {
    is_name_byte(b) || b == b'-'
}

fn scan(text: &str, extra: &dyn Fn(&str) -> bool) -> Vec<Placeholder> {
    let bytes = text.as_bytes();
    let assigned = assigned_names(text);
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &bytes[i..];
        let token: Option<(usize, usize, usize, bool)> = match rest[0] {
            b'$' if rest.get(1) == Some(&b'{') => {
                let end = rest[2..]
                    .iter()
                    .position(|b| !is_name_byte(*b))
                    .map(|p| p + 2)
                    .unwrap_or(rest.len());
                (end > 2 && rest.get(end) == Some(&b'}')).then_some((end + 1, 2, end, true))
            }
            b'$' => {
                let end = rest[1..]
                    .iter()
                    .position(|b| !is_name_byte(*b))
                    .map(|p| p + 1)
                    .unwrap_or(rest.len());
                (end > 1).then_some((end, 1, end, true))
            }
            b'<' => {
                let end = rest[1..]
                    .iter()
                    .position(|b| !is_dash_name_byte(*b))
                    .map(|p| p + 1)
                    .unwrap_or(rest.len());
                (end > 1 && rest.get(end) == Some(&b'>')).then_some((end + 1, 1, end, false))
            }
            b'{' if rest.starts_with(b"{{") => {
                let end = rest[2..]
                    .iter()
                    .position(|b| !is_dash_name_byte(*b))
                    .map(|p| p + 2)
                    .unwrap_or(rest.len());
                (end > 2 && rest[end..].starts_with(b"}}")).then_some((end + 2, 2, end, false))
            }
            b'[' => {
                let end = rest[1..]
                    .iter()
                    .position(|b| !is_dash_name_byte(*b))
                    .map(|p| p + 1)
                    .unwrap_or(rest.len());
                let wiki_or_link = i > 0 && bytes[i - 1] == b'['
                    || rest.get(end + 1) == Some(&b']')
                    || rest.get(end + 1) == Some(&b'(');
                (end > 1 && rest.get(end) == Some(&b']') && !wiki_or_link).then_some((
                    end + 1,
                    1,
                    end,
                    false,
                ))
            }
            _ => None,
        };
        let Some((length, name_start, name_end, shell_form)) = token else {
            i += 1;
            continue;
        };
        let name = &text[i + name_start..i + name_end];
        let normalized = normalize(name);
        let excluded = shell_form
            && (SHELL_VARIABLES.contains(&normalized.as_str()) || assigned.contains(&normalized));
        if let Some(canonical) = (!excluded).then(|| canonical(name, extra)).flatten() {
            found.push(Placeholder {
                range: i..i + length,
                name: canonical,
            });
            i += length;
        } else {
            i += 1;
        }
    }
    found
}

pub fn find(text: &str) -> Vec<Placeholder> {
    scan(text, &|_| false)
}

pub const MASK: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

pub fn substitute_masked(text: &str, context: &Context) -> Substituted {
    substitute_with(text, context, true)
}

pub fn substitute(text: &str, context: &Context) -> Substituted {
    substitute_with(text, context, false)
}

fn substitute_with(text: &str, context: &Context, hide_secrets: bool) -> Substituted {
    let known = |name: &str| context.values.contains_key(name);
    let mut out = Substituted::default();
    let mut cursor = 0;
    for placeholder in scan(text, &known) {
        out.text.push_str(&text[cursor..placeholder.range.start]);
        cursor = placeholder.range.end;
        match context
            .values
            .get(&placeholder.name)
            .filter(|v| !v.is_empty())
        {
            Some(value) => {
                let start = out.text.len();
                out.text.push_str(
                    if hide_secrets && context.secret.contains(&placeholder.name) {
                        MASK
                    } else {
                        value
                    },
                );
                out.resolved.push((start..out.text.len(), placeholder.name));
                out.written
                    .push(text[placeholder.range.clone()].to_string());
            }
            None => {
                let start = out.text.len();
                out.text.push_str(&text[placeholder.range.clone()]);
                out.missing.push((start..out.text.len(), placeholder.name));
            }
        }
    }
    out.text.push_str(&text[cursor..]);
    out
}

#[cfg(unix)]
fn interface_ipv4(name: &str) -> Option<String> {
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `getifaddrs` fills `list` with a linked list that is only read, then freed with
    // `freeifaddrs`. Each pointer is checked for null and `sin_addr` is read only for `AF_INET`.
    unsafe {
        if libc::getifaddrs(&mut list) != 0 {
            return None;
        }
        let mut found = None;
        let mut current = list;
        while !current.is_null() {
            let entry = &*current;
            if !entry.ifa_addr.is_null() && i32::from((*entry.ifa_addr).sa_family) == libc::AF_INET
            {
                let interface = std::ffi::CStr::from_ptr(entry.ifa_name).to_string_lossy();
                if interface == name {
                    let address = &*(entry.ifa_addr as *const libc::sockaddr_in);
                    found = Some(
                        std::net::Ipv4Addr::from(u32::from_be(address.sin_addr.s_addr)).to_string(),
                    );
                    break;
                }
            }
            current = entry.ifa_next;
        }
        libc::freeifaddrs(list);
        found
    }
}

#[cfg(windows)]
fn interface_ipv4(name: &str) -> Option<String> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
        GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};
    const ERROR_BUFFER_OVERFLOW: u32 = 111;
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size: u32 = 16 * 1024;
    let mut buffer: Vec<u64> = Vec::new();
    let mut filled = false;
    for _ in 0..3 {
        buffer.resize((size as usize).div_ceil(8), 0);
        // SAFETY: `buffer` holds `size` bytes, as the call is told.
        let result = unsafe {
            GetAdaptersAddresses(
                u32::from(AF_INET),
                flags,
                std::ptr::null(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        match result {
            0 => {
                filled = true;
                break;
            }
            ERROR_BUFFER_OVERFLOW => continue,
            _ => return None,
        }
    }
    if !filled {
        return None;
    }
    // SAFETY: the system wrote a linked list of adapters into `buffer`, which outlives this
    // walk; each pointer is checked for null and an address is read as IPv4 only for `AF_INET`.
    unsafe {
        let mut adapter = buffer.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        while !adapter.is_null() {
            let entry = &*adapter;
            let shown = if entry.FriendlyName.is_null() {
                String::new()
            } else {
                let length = (0..)
                    .take_while(|&i| *entry.FriendlyName.add(i) != 0)
                    .count();
                String::from_utf16_lossy(std::slice::from_raw_parts(entry.FriendlyName, length))
            };
            let id = if entry.AdapterName.is_null() {
                String::new()
            } else {
                std::ffi::CStr::from_ptr(entry.AdapterName.cast())
                    .to_string_lossy()
                    .into_owned()
            };
            if shown == name || id == name {
                let mut unicast = entry.FirstUnicastAddress;
                while !unicast.is_null() {
                    let address = (*unicast).Address.lpSockaddr;
                    if !address.is_null() && (*address).sa_family == AF_INET {
                        let ipv4 = &*address.cast::<SOCKADDR_IN>();
                        return Some(
                            std::net::Ipv4Addr::from(u32::from_be(ipv4.sin_addr.S_un.S_addr))
                                .to_string(),
                        );
                    }
                    unicast = (*unicast).Next;
                }
            }
            adapter = entry.Next;
        }
    }
    None
}

pub fn context_for(engagement: &super::Engagement, focus: Option<&super::Host>) -> Context {
    context_for_settings(engagement, focus, "tun0", &[])
}

pub fn context_for_settings(
    engagement: &super::Engagement,
    focus: Option<&super::Host>,
    interface: &str,
    defaults: &[(String, String)],
) -> Context {
    context_with(engagement, focus, interface_ipv4(interface), defaults)
}

fn context_with(
    engagement: &super::Engagement,
    focus: Option<&super::Host>,
    tunnel_address: Option<String>,
    defaults: &[(String, String)],
) -> Context {
    let mut values: BTreeMap<String, String> = BTreeMap::new();
    let mut aliased: Vec<(String, &String)> = Vec::new();
    for (name, value) in &engagement.vars {
        let normalized = normalize(name);
        match alias_target(&normalized) {
            Some(canonical) if canonical == normalized => {
                values.insert(normalized, value.clone());
            }
            Some(canonical) => aliased.push((canonical.to_string(), value)),
            None => {
                values.insert(normalized, value.clone());
            }
        }
    }
    for (canonical, value) in aliased {
        values.entry(canonical).or_insert_with(|| value.clone());
    }
    if let Some(host) = focus {
        values.insert("RHOST".into(), host.ip.clone());
        if let Some(name) = &host.hostname {
            values.insert("HOSTNAME".into(), name.clone());
        }
    }
    for (name, value) in defaults {
        let normalized = normalize(name);
        let canonical = alias_target(&normalized).map_or(normalized, str::to_string);
        values.entry(canonical).or_insert_with(|| value.clone());
    }
    if let Some(address) = tunnel_address {
        values.entry("LHOST".into()).or_insert(address);
    }
    let secret = engagement
        .var_links
        .keys()
        .filter(|name| crate::links::is_secret(name))
        .cloned()
        .collect();
    Context { values, secret }
}

const EXEGOL: &[(&str, &str)] = &[
    ("TARGET", "RHOST"),
    ("DOMAIN", "DOMAIN"),
    ("USER", "USER"),
    ("PASSWORD", "PASS"),
    ("NT_HASH", "HASH"),
    ("DC_IP", "DC"),
    ("DC_HOST", "DC_HOST"),
    ("DOMAIN_SID", "DOMAIN_SID"),
    ("ATTACKER_IP", "LHOST"),
    ("INTERFACE", "INTERFACE"),
];

pub fn display_name(canonical: &str) -> &str {
    EXEGOL
        .iter()
        .find(|(_, internal)| *internal == canonical)
        .map_or(canonical, |(exegol, _)| exegol)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standard {
    pub name: &'static str,
    pub key: &'static str,
    pub description_key: String,
    pub aliases: Vec<&'static str>,
}

pub fn standard() -> Vec<Standard> {
    let mut list: Vec<Standard> = EXEGOL
        .iter()
        .map(|(name, internal)| {
            let aliases = BUILTIN
                .iter()
                .find(|(canonical, _, _)| canonical == internal)
                .map(|(canonical, aliases, _)| {
                    std::iter::once(*canonical)
                        .chain(aliases.iter().copied())
                        .filter(|a| a != name)
                        .collect()
                })
                .unwrap_or_default();
            Standard {
                name,
                key: internal,
                description_key: format!("ops-var-{}-desc", internal.to_lowercase()),
                aliases,
            }
        })
        .collect();
    for (canonical, aliases, _) in BUILTIN {
        if !EXEGOL.iter().any(|(_, internal)| internal == canonical) {
            list.push(Standard {
                name: canonical,
                key: canonical,
                description_key: format!("ops-var-{}-desc", canonical.to_lowercase()),
                aliases: aliases.to_vec(),
            });
        }
    }
    list
}

pub fn known() -> &'static [(&'static str, &'static str)] {
    static KNOWN: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
        BUILTIN
            .iter()
            .map(|(name, _, description)| (*name, *description))
            .collect()
    });
    &KNOWN
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(pairs: &[(&str, &str)]) -> Context {
        Context {
            values: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    fn names(text: &str) -> Vec<String> {
        find(text).into_iter().map(|p| p.name).collect()
    }

    #[test]
    fn the_exegol_names_and_the_earlier_ones_read_the_same_variable() {
        let ctx = context(&[
            ("RHOST", "10.0.0.5"),
            ("PASS", "pw"),
            ("HASH", "aad3"),
            ("DC", "10.0.0.1"),
            ("LHOST", "10.10.14.2"),
        ]);
        let text =
            "$TARGET $RHOST ${PASSWORD} $PASS $NT_HASH $DC_IP $DC $ATTACKER_IP $LHOST $DOMAIN_SID";
        let result = substitute(text, &ctx);
        assert_eq!(
            result.text,
            "10.0.0.5 10.0.0.5 pw pw aad3 10.0.0.1 10.0.0.1 10.10.14.2 10.10.14.2 $DOMAIN_SID"
        );
        assert_eq!(
            result.missing.len(),
            1,
            "a standard name without a value stays marked"
        );
    }

    #[test]
    fn the_standard_list_puts_the_exegol_names_first_with_their_aliases() {
        let list = standard();
        let names: Vec<&str> = list.iter().map(|s| s.name).take(10).collect();
        assert_eq!(
            names,
            [
                "TARGET",
                "DOMAIN",
                "USER",
                "PASSWORD",
                "NT_HASH",
                "DC_IP",
                "DC_HOST",
                "DOMAIN_SID",
                "ATTACKER_IP",
                "INTERFACE"
            ]
        );
        let password = list.iter().find(|s| s.name == "PASSWORD").unwrap();
        assert!(
            password.aliases.contains(&"PASS")
                && password.aliases.contains(&"PWD")
                && password.key == "PASS"
        );
        assert!(
            list.iter().any(|s| s.name == "LPORT")
                && display_name("RHOST") == "TARGET"
                && display_name("LPORT") == "LPORT"
        );
    }

    #[test]
    fn all_five_forms_and_aliases() {
        assert_eq!(names("ssh $USER@$IP"), ["USER", "RHOST"]);
        assert_eq!(names("ssh ${user}@${target_ip}"), ["USER", "RHOST"]);
        assert_eq!(
            names("ssh <USERNAME>@<RHOSTS> -p <port>"),
            ["USER", "RHOST", "LPORT"]
        );
        assert_eq!(
            names("curl {{url}} -u {{user}}:{{password}}"),
            ["URL", "USER", "PASS"]
        );
        assert_eq!(names("nc [LHOST] [lport]"), ["LHOST", "LPORT"]);
        assert_eq!(
            names(
                "ping <dc-ip> <Kali> <NTHASH> <iface> <dom> <host> <victim> <attacker> <wordlist>"
            ),
            [
                "DC",
                "LHOST",
                "HASH",
                "INTERFACE",
                "DOMAIN",
                "HOSTNAME",
                "RHOST",
                "LHOST",
                "WORDLIST"
            ]
        );
        let text = "a $IP b";
        assert_eq!(find(text)[0].range, 2..5);
    }

    #[test]
    fn shell_variables_are_never_context() {
        for text in [
            "cd $HOME && ls $PATH",
            "echo $1 $2 $? $$ $! $@ $# $_ $-",
            "echo $PWD ${PWD} $SHELL $RANDOM $IFS ${HOME}/bin $UID",
            "echo ${IP:-10.0.0.1} ${USER:?}",
            "export PATH=$PATH:/opt/bin",
            "echo $IP_ADDRESS $USERS $TARGETS",
        ] {
            assert!(names(text).is_empty(), "{text} : {:?}", names(text));
        }
        assert_eq!(names("login <PWD>"), ["PASS"]);
    }

    #[test]
    fn names_assigned_in_the_text_are_shell_variables_there() {
        assert!(names("USER=root\necho $USER").is_empty());
        assert!(names("export IP=10.0.0.5; ping $IP").is_empty());
        assert!(names("for user in a b c; do id $user; done").is_empty());
        assert!(names("while read -r pass; do echo $pass; done < list.txt").is_empty());
        assert_eq!(names("USER=root\necho <USER> $IP"), ["USER", "RHOST"]);
        assert_eq!(
            names("tool --user=bob $USER"),
            ["USER"],
            "an option is not an assignment"
        );
    }

    #[test]
    fn brackets_in_markdown_and_regexes_are_left_alone() {
        assert!(names("see [[IP]] and [URL](https://example.test) and [DC]: anchor").len() <= 1);
        assert!(names("[[IP]]").is_empty());
        assert!(names("[URL](https://example.test)").is_empty());
        assert!(names("a < b > c <> <1> <html> [x] [a-z] {{}} {{ ip }}").is_empty());
        assert!(
            names("$").is_empty()
                && names("${").is_empty()
                && names("<IP").is_empty()
                && names("{{IP}").is_empty()
        );
    }

    #[test]
    fn engagement_variables_are_known_by_normalised_name() {
        let ctx = context(&[("MY_VAR", "value")]);
        let result = substitute("echo $my_var {{my-var}} <MY_VAR> $other", &ctx);
        assert_eq!(result.text, "echo value value value $other");
        assert_eq!(result.resolved.len(), 3);
        assert!(result.missing.is_empty(), "$other is not a known variable");
    }

    #[test]
    fn a_command_of_the_library_resolves_whatever_the_case_of_its_names() {
        let ctx = context(&[
            ("DOMAIN", "test.fr"),
            ("DC", "10.0.0.1"),
            ("USER", "admin"),
            ("PASS", "pw"),
            ("MY_VAR", "x"),
        ]);
        let command = "nxc smb \"$dc\" -d \"${Domain}\" -u \"$USER\" -p \"$PASS\" --pass-pol | tee logs/password-policy.txt $my_var";
        let result = substitute(command, &ctx);
        assert_eq!(
            result.text,
            "nxc smb \"10.0.0.1\" -d \"test.fr\" -u \"admin\" -p \"pw\" --pass-pol | tee logs/password-policy.txt x"
        );
        assert_eq!(
            result.written,
            ["$dc", "${Domain}", "$USER", "$PASS", "$my_var"]
        );
        assert!(result.missing.is_empty());
    }

    #[test]
    fn a_secret_variable_is_masked_on_screen_and_clear_in_what_is_copied() {
        let mut ctx = context(&[("USER", "alice"), ("PASS", "Winter2026!")]);
        ctx.secret.insert("PASS".into());
        let command = "nxc smb 10.0.0.1 -u $USER -p '$PASS'";
        let shown = substitute_masked(command, &ctx);
        assert_eq!(shown.text, format!("nxc smb 10.0.0.1 -u alice -p '{MASK}'"));
        assert!(!shown.text.contains("Winter2026!") && shown.resolved.len() == 2);
        assert!(substitute(command, &ctx).text.contains("-p 'Winter2026!'"));
        assert_eq!(
            &shown.text[shown.resolved[1].0.clone()],
            MASK,
            "the range covers the mask"
        );
    }

    #[test]
    fn substitute_reports_positions_in_the_output() {
        let ctx = context(&[("RHOST", "192.0.2.5"), ("USER", "")]);
        let result = substitute("ssh $USER@$IP -p <PORT>", &ctx);
        assert_eq!(result.text, "ssh $USER@192.0.2.5 -p <PORT>");
        assert_eq!(result.resolved, [(10..19, "RHOST".to_string())]);
        assert_eq!(
            result.missing,
            [(4..9, "USER".to_string()), (23..29, "LPORT".to_string())]
        );
        for (range, _) in result.resolved.iter().chain(&result.missing) {
            assert!(
                result.text.is_char_boundary(range.start)
                    && result.text.is_char_boundary(range.end)
            );
        }
        let unicode = substitute("echo $IP é", &ctx);
        assert_eq!(unicode.text, "echo 192.0.2.5 é");
        assert_eq!(&unicode.text[unicode.resolved[0].0.clone()], "192.0.2.5");
    }

    #[test]
    fn context_combines_vars_focus_and_tunnel() {
        let engagement = crate::Engagement {
            dir: Default::default(),
            name: "x".into(),
            kind: crate::EngagementKind::Lab,
            status: crate::EngagementStatus::InProgress,
            client: None,
            started: None,
            deadline: None,
            scope: vec![],
            out_of_scope: vec![],
            var_links: Default::default(),
            pauses: vec![],
            stopped: None,
            vars: [
                ("target", "192.0.2.1"),
                ("RHOST", "192.0.2.2"),
                ("password", "secret"),
                ("custom-name", "v"),
                ("LHOST", "192.0.2.9"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            exam: None,
            exam_done: vec![],
            focus: None,
        };
        let host = crate::Host {
            path: Default::default(),
            ip: "192.0.2.77".into(),
            hostname: Some("srv".into()),
            os: None,
            status: crate::HostStatus::New,
            services: vec![],
            tags: vec![],
            flags: vec![],
            points: None,
        };
        let plain = context_with(&engagement, None, Some("198.51.100.1".into()), &[]);
        assert_eq!(
            plain.values.get("RHOST").map(String::as_str),
            Some("192.0.2.2"),
            "the canonical name wins over the alias"
        );
        assert_eq!(plain.values.get("PASS").map(String::as_str), Some("secret"));
        assert_eq!(
            plain.values.get("CUSTOM_NAME").map(String::as_str),
            Some("v")
        );
        assert_eq!(
            plain.values.get("LHOST").map(String::as_str),
            Some("192.0.2.9"),
            "tun0 does not replace a typed value"
        );
        let focused = context_with(&engagement, Some(&host), Some("198.51.100.1".into()), &[]);
        assert_eq!(
            (
                focused.values["RHOST"].as_str(),
                focused.values["HOSTNAME"].as_str()
            ),
            ("192.0.2.77", "srv")
        );
        let mut without_lhost = engagement.clone();
        without_lhost.vars.remove("LHOST");
        assert_eq!(
            context_with(&without_lhost, None, Some("198.51.100.1".into()), &[]).values["LHOST"],
            "198.51.100.1"
        );
        assert!(
            !context_with(&without_lhost, None, None, &[])
                .values
                .contains_key("LHOST")
        );
        let defaults = [
            ("domain".to_string(), "corp.local".to_string()),
            ("lhost".to_string(), "10.10.14.2".to_string()),
            ("password".to_string(), "ignored".to_string()),
        ];
        let with_defaults =
            context_with(&without_lhost, None, Some("198.51.100.1".into()), &defaults);
        assert_eq!(with_defaults.values["DOMAIN"], "corp.local");
        assert_eq!(
            with_defaults.values["LHOST"], "10.10.14.2",
            "a default wins over the interface address"
        );
        assert_eq!(
            with_defaults.values["PASS"], "secret",
            "the engagement wins over a default"
        );
        let _ = interface_ipv4("tun0");
    }

    #[test]
    fn known_lists_every_canonical_name_once() {
        let listed: Vec<&str> = known().iter().map(|(n, _)| *n).collect();
        assert_eq!(
            listed,
            [
                "RHOST",
                "LHOST",
                "LPORT",
                "DOMAIN",
                "USER",
                "PASS",
                "HASH",
                "DC",
                "DC_HOST",
                "DOMAIN_SID",
                "URL",
                "WORDLIST",
                "HOSTNAME",
                "INTERFACE"
            ]
        );
        assert!(known().iter().all(|(_, d)| !d.is_empty()));
    }

    #[test]
    fn real_command_blocks_from_the_vault() {
        let Some(root) = std::env::var_os("FOCUS_REAL_VAULT").map(std::path::PathBuf::from) else {
            return;
        };
        if !root.is_dir() {
            return;
        }
        let mut files = vec![root];
        let (mut blocks, mut placeholders, mut shell_vars_seen) = (0usize, 0usize, 0usize);
        while let Some(path) = files.pop() {
            if path.is_dir() {
                files.extend(
                    std::fs::read_dir(&path)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .map(|e| e.path()),
                );
                continue;
            }
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let mut inside = false;
            let mut block = String::new();
            for line in text.lines() {
                if line.trim_start().starts_with("```") {
                    if inside && !block.is_empty() {
                        blocks += 1;
                        for p in find(&block) {
                            placeholders += 1;
                            let token = &block[p.range.clone()];
                            if token.starts_with('$') {
                                let bare = token
                                    .trim_start_matches('$')
                                    .trim_start_matches('{')
                                    .trim_end_matches('}')
                                    .to_ascii_uppercase();
                                assert!(
                                    !SHELL_VARIABLES.contains(&bare.as_str())
                                        && !bare.chars().all(|c| c.is_ascii_digit()),
                                    "{token} taken for a placeholder in {}",
                                    path.display()
                                );
                            }
                        }
                        shell_vars_seen += block.matches("$PATH").count()
                            + block.matches("$HOME").count()
                            + block.matches("$1").count();
                    }
                    inside = !inside;
                    block.clear();
                } else if inside {
                    block.push_str(line);
                    block.push('\n');
                }
            }
        }
        assert!(blocks > 100, "too few blocks read: {blocks}");
        println!(
            "{blocks} blocks, {placeholders} placeholders, {shell_vars_seen} shell variables excluded"
        );
    }

    #[test]
    fn a_write_reuses_the_key_the_readme_has() {
        let mut vars: BTreeMap<String, String> = [
            ("TARGET".to_string(), "10.0.0.5".to_string()),
            ("PASSWORD".to_string(), "old".to_string()),
        ]
        .into();
        store(&mut vars, "RHOST", "10.0.0.6");
        store(&mut vars, "PASS", "new");
        assert_eq!(vars.len(), 2, "{vars:?}");
        assert_eq!(
            (vars["TARGET"].as_str(), vars["PASSWORD"].as_str()),
            ("10.0.0.6", "new")
        );
        store(&mut vars, "pass", "");
        assert!(!vars.contains_key("PASSWORD"));
        store(&mut vars, "DC", "10.0.0.1");
        assert_eq!(vars["DC"], "10.0.0.1");
    }

    #[test]
    fn a_write_removes_the_second_key_of_a_variable() {
        let mut vars: BTreeMap<String, String> = [
            ("DC".to_string(), "a".to_string()),
            ("DC_IP".to_string(), "b".to_string()),
        ]
        .into();
        store(&mut vars, "DC_IP", "c");
        assert_eq!(vars.len(), 1, "{vars:?}");
        assert_eq!(stored(&vars, "DC").map(|(_, v)| v.as_str()), Some("c"));
    }
}
