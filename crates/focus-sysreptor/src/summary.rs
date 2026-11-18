use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Journal,
    Checklist,
    ExamMilestones,
    Variables,
    HostNotes,
    Credentials,
    NoFieldInDesign,
    CweNotListed,
    CweExtra,
    CvssNotUsable,
    ImageMissing,
    ImageRefused,
    NoteEmbed,
    SectionMerged,
    ImageRemote,
    ImagesWrittenAside,
    CommentsRemoved,
}

impl Kind {
    pub fn is_loss(self) -> bool {
        !matches!(
            self,
            Kind::SectionMerged
                | Kind::ImageRemote
                | Kind::ImagesWrittenAside
                | Kind::CommentsRemoved
        )
    }

    pub fn key(self) -> &'static str {
        match self {
            Kind::Journal => "journal",
            Kind::Checklist => "checklist",
            Kind::ExamMilestones => "exam-milestones",
            Kind::Variables => "variables",
            Kind::HostNotes => "host-notes",
            Kind::Credentials => "credentials",
            Kind::NoFieldInDesign => "no-field-in-design",
            Kind::CweNotListed => "cwe-not-listed",
            Kind::CweExtra => "cwe-extra",
            Kind::CvssNotUsable => "cvss-not-usable",
            Kind::ImageMissing => "image-missing",
            Kind::ImageRefused => "image-refused",
            Kind::NoteEmbed => "note-embed",
            Kind::SectionMerged => "section-merged",
            Kind::ImageRemote => "image-remote",
            Kind::ImagesWrittenAside => "images-written-aside",
            Kind::CommentsRemoved => "comments-removed",
        }
    }

    fn text(self) -> &'static str {
        match self {
            Kind::Journal => "journal entries are not exported",
            Kind::Checklist => "checklist progress is not exported",
            Kind::ExamMilestones => "exam milestones are not exported",
            Kind::Variables => "engagement variables are never exported",
            Kind::HostNotes => "host note texts are not exported",
            Kind::Credentials => "credentials are not exported",
            Kind::NoFieldInDesign => "no field in the target design",
            Kind::CweNotListed => "CWE not in the list SysReptor accepts",
            Kind::CweExtra => "further CWE, the SysReptor field holds one",
            Kind::CvssNotUsable => "CVSS vector SysReptor cannot use",
            Kind::ImageMissing => "image not found",
            Kind::ImageRefused => "image refused",
            Kind::NoteEmbed => "embedded note removed",
            Kind::SectionMerged => "section written into the description",
            Kind::ImageRemote => "remote image kept as a link",
            Kind::ImagesWrittenAside => "images written next to the finding files",
            Kind::CommentsRemoved => "comments removed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub kind: Kind,
    pub subject: Option<String>,
    pub count: usize,
    pub detail: String,
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kind.text())?;
        if let Some(subject) = &self.subject {
            write!(f, ": {subject}")?;
        }
        if !self.detail.is_empty() {
            write!(f, " ({})", self.detail)?;
        } else if self.count > 1 {
            write!(f, " ({})", self.count)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub design: String,
    pub findings: usize,
    pub images: usize,
    pub masked: usize,
    pub proofs: usize,
    pub left_out: Vec<Entry>,
    pub adjusted: Vec<Entry>,
}

impl Summary {
    pub(crate) fn note(&mut self, kind: Kind, subject: Option<&str>, detail: impl Into<String>) {
        self.push(Entry {
            kind,
            subject: subject.map(str::to_string),
            count: 1,
            detail: detail.into(),
        });
    }

    pub(crate) fn note_count(&mut self, kind: Kind, subject: Option<&str>, count: usize) {
        self.push(Entry {
            kind,
            subject: subject.map(str::to_string),
            count,
            detail: String::new(),
        });
    }

    fn push(&mut self, entry: Entry) {
        if entry.kind.is_loss() {
            self.left_out.push(entry)
        } else {
            self.adjusted.push(entry)
        }
    }

    pub fn to_text(&self) -> String {
        let mut out = format!(
            "Design: {}\nFindings: {}\nImages attached: {}\nSecrets masked: {}\nProofs: {}\n",
            self.design, self.findings, self.images, self.masked, self.proofs
        );
        for (title, entries) in [("Left out", &self.left_out), ("Adjusted", &self.adjusted)] {
            if !entries.is_empty() {
                out.push_str(&format!("\n{title}:\n"));
                for entry in entries {
                    out.push_str(&format!("- {entry}\n"));
                }
            }
        }
        out
    }
}
