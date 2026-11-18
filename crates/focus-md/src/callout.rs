pub fn callout_kind(kind: &str) -> (&'static str, String) {
    let lower = kind.trim().to_lowercase();
    let known: Option<(&'static str, &'static str)> = match lower.as_str() {
        "note" => Some(("note", "Note")),
        "abstract" => Some(("abstract", "Summary")),
        "summary" => Some(("abstract", "Summary")),
        "tldr" => Some(("abstract", "TL;DR")),
        "info" => Some(("info", "Info")),
        "todo" => Some(("todo", "To do")),
        "tip" => Some(("tip", "Tip")),
        "hint" => Some(("tip", "Hint")),
        "important" => Some(("tip", "Important")),
        "success" => Some(("success", "Success")),
        "check" => Some(("success", "Check")),
        "done" => Some(("success", "Done")),
        "question" => Some(("question", "Question")),
        "help" => Some(("question", "Help")),
        "faq" => Some(("question", "FAQ")),
        "warning" => Some(("warning", "Warning")),
        "caution" => Some(("warning", "Caution")),
        "attention" => Some(("warning", "Attention")),
        "failure" => Some(("failure", "Failure")),
        "fail" => Some(("failure", "Fail")),
        "missing" => Some(("failure", "Missing")),
        "danger" => Some(("danger", "Danger")),
        "error" => Some(("danger", "Error")),
        "bug" => Some(("bug", "Bug")),
        "example" => Some(("example", "Exemple")),
        "quote" => Some(("quote", "Quote")),
        "cite" => Some(("quote", "Cite")),
        _ => None,
    };
    match known {
        Some((family, label)) => (family, label.to_string()),
        None => ("note", kind.trim().to_string()),
    }
}
