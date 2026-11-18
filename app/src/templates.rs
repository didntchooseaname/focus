pub type HostTemplate = (&'static str, &'static str);

const EN: &[HostTemplate] = &[
    (
        "exam-journal-bscp",
        include_str!("../../assets/templates/en/exam-journal-bscp.md"),
    ),
    (
        "exam-journal-cpts",
        include_str!("../../assets/templates/en/exam-journal-cpts.md"),
    ),
    (
        "exam-journal-crte",
        include_str!("../../assets/templates/en/exam-journal-crte.md"),
    ),
    (
        "exam-journal-crto",
        include_str!("../../assets/templates/en/exam-journal-crto.md"),
    ),
    (
        "exam-journal-crtp",
        include_str!("../../assets/templates/en/exam-journal-crtp.md"),
    ),
    (
        "exam-journal-ecppt",
        include_str!("../../assets/templates/en/exam-journal-ecppt.md"),
    ),
    (
        "exam-journal-ejpt",
        include_str!("../../assets/templates/en/exam-journal-ejpt.md"),
    ),
    (
        "exam-journal-oscp",
        include_str!("../../assets/templates/en/exam-journal-oscp.md"),
    ),
    (
        "exam-journal-osed",
        include_str!("../../assets/templates/en/exam-journal-osed.md"),
    ),
    (
        "exam-journal-osep",
        include_str!("../../assets/templates/en/exam-journal-osep.md"),
    ),
    (
        "exam-journal-oswe",
        include_str!("../../assets/templates/en/exam-journal-oswe.md"),
    ),
    (
        "exam-journal-pnpt",
        include_str!("../../assets/templates/en/exam-journal-pnpt.md"),
    ),
    (
        "machine-sheet",
        include_str!("../../assets/templates/en/machine-sheet.md"),
    ),
    (
        "prep-tracker",
        include_str!("../../assets/templates/en/prep-tracker.md"),
    ),
    (
        "report-outline",
        include_str!("../../assets/templates/en/report-outline.md"),
    ),
];

const FR: &[HostTemplate] = &[
    (
        "exam-journal-bscp",
        include_str!("../../assets/templates/fr/exam-journal-bscp.md"),
    ),
    (
        "exam-journal-cpts",
        include_str!("../../assets/templates/fr/exam-journal-cpts.md"),
    ),
    (
        "exam-journal-crte",
        include_str!("../../assets/templates/fr/exam-journal-crte.md"),
    ),
    (
        "exam-journal-crto",
        include_str!("../../assets/templates/fr/exam-journal-crto.md"),
    ),
    (
        "exam-journal-crtp",
        include_str!("../../assets/templates/fr/exam-journal-crtp.md"),
    ),
    (
        "exam-journal-ecppt",
        include_str!("../../assets/templates/fr/exam-journal-ecppt.md"),
    ),
    (
        "exam-journal-ejpt",
        include_str!("../../assets/templates/fr/exam-journal-ejpt.md"),
    ),
    (
        "exam-journal-oscp",
        include_str!("../../assets/templates/fr/exam-journal-oscp.md"),
    ),
    (
        "exam-journal-osed",
        include_str!("../../assets/templates/fr/exam-journal-osed.md"),
    ),
    (
        "exam-journal-osep",
        include_str!("../../assets/templates/fr/exam-journal-osep.md"),
    ),
    (
        "exam-journal-oswe",
        include_str!("../../assets/templates/fr/exam-journal-oswe.md"),
    ),
    (
        "exam-journal-pnpt",
        include_str!("../../assets/templates/fr/exam-journal-pnpt.md"),
    ),
    (
        "machine-sheet",
        include_str!("../../assets/templates/fr/machine-sheet.md"),
    ),
    (
        "prep-tracker",
        include_str!("../../assets/templates/fr/prep-tracker.md"),
    ),
    (
        "report-outline",
        include_str!("../../assets/templates/fr/report-outline.md"),
    ),
];

pub fn host_templates() -> &'static [HostTemplate] {
    if focus_i18n::language() == "fr" {
        FR
    } else {
        EN
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_template_is_built_in_both_languages_with_its_variables() {
        assert_eq!((super::EN.len(), super::FR.len()), (15, 15));
        for (name, text) in super::EN.iter().chain(super::FR) {
            assert!(
                text.contains("{{date}}") || text.contains("{{title}}"),
                "{name} has no variable"
            );
        }
    }
}
