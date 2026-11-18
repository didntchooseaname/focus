use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeTone {
    Neutral,
    Ok,
    Warn,
    Danger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportFormat {
    Html,
    Pdf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpsPage {
    Overview,
    Hosts,
    Findings,
    Credentials,
    Commands,
    Report,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShowPage(pub OpsPage);

#[derive(Clone, Debug, PartialEq)]
pub enum OpsEvent {
    OpenNote {
        path: PathBuf,
        line: Option<u32>,
    },
    ExportJournalCsv {
        file_name: String,
        content: String,
    },
    ActiveChanged(PathBuf),
    EngagementCreated(PathBuf),
    AttachEvidence {
        directory: PathBuf,
        name_prefix: String,
    },
    ExportReport {
        path: PathBuf,
        format: ReportFormat,
    },
    OpenLink {
        target: String,
    },
    ZoomDiagram(String),
    HoverLink {
        target: Option<String>,
        position: gpui::Point<gpui::Pixels>,
    },
    SearchTag(String),
    DashboardRequested,
    DialogClosed,
    NewEngagementRequested,
    Notify {
        tone: NoticeTone,
        text: String,
    },
}
