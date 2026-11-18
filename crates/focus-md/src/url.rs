pub(crate) fn url_scheme(url: &str) -> Option<String> {
    let clean: String = url
        .chars()
        .filter(|c| !c.is_control() && !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    let end = clean.find([':', '/', '?', '#'])?;
    (clean.as_bytes()[end] == b':').then(|| clean[..end].to_string())
}

pub fn is_safe_url(url: &str, image: bool) -> bool {
    match url_scheme(url).as_deref() {
        None | Some("http" | "https" | "mailto" | "tel") => true,
        Some("data") => {
            image
                && url
                    .chars()
                    .filter(|c| !c.is_control() && !c.is_whitespace())
                    .flat_map(char::to_lowercase)
                    .collect::<String>()
                    .starts_with("data:image/")
        }
        Some(_) => false,
    }
}

pub(crate) fn local_url(url: String) -> String {
    if url
        .split('/')
        .next()
        .is_some_and(|first| first.contains(':'))
    {
        format!("./{url}")
    } else {
        url
    }
}
