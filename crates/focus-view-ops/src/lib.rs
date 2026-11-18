mod commands;
mod credentials;
mod dashboard;
mod dialog;
mod dir;
mod event;
mod findings;
mod hosts;
mod indicator;
mod journal;
mod report;
mod resolve;
mod rich;
mod settings;
mod sidebar;
mod state;
mod time;
mod ui;

pub use commands::CommandLibrary;
pub use credentials::CredentialsTable;
pub use dashboard::OpsDashboard;
pub use dialog::NewEngagementDialog;
pub use event::{NoticeTone, OpsEvent, OpsPage, ReportFormat, ShowPage};
pub use findings::FindingsList;
pub use hosts::HostsTable;
pub use indicator::OpsIndicator;
pub use journal::JournalPanel;
pub use report::ReportView;
pub use resolve::{code_block_variables, variable_highlights};
pub use settings::{OpsSettings, parse_variables};
pub use sidebar::OpsSidebar;
pub use state::{
    Active, EngagementTrash, Mover, OpsState, StateEvent, graph_relations, load_active,
};

#[cfg(test)]
mod key_tests {
    #[test]
    fn every_key_used_in_the_sources_exists_in_english() {
        let english = focus_i18n::audit::embedded("en");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            for prefix in ["t_(\"", "t_args(\"", "key: \""] {
                for part in text.split(prefix).skip(1) {
                    let key = part.split('"').next().unwrap();
                    if (key.starts_with("ops-") || key.starts_with("errors-ops-"))
                        && !key.ends_with('-')
                        && !english.messages.contains_key(key)
                    {
                        missing.push(key.to_string());
                    }
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "keys missing from locales/en/ops.ftl: {missing:?}"
        );
    }
}
