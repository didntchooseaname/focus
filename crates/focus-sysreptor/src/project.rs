use serde_json::{Value, json};

use crate::Options;
use crate::design::{self, Fields, SECTION_IDS};
use crate::export::{Prepared, project_findings, report_data};
use crate::ids::uuid;
use crate::mask::Masker;
use crate::summary::Summary;

pub(crate) fn project_id(prepared: &Prepared) -> String {
    uuid(&["focus-sysreptor-project", &prepared.engagement.name])
}

pub(crate) fn project(
    prepared: &Prepared,
    options: &Options,
    masker: &mut Masker,
    summary: &mut Summary,
) -> Value {
    let data = report_data(prepared, options, masker);
    let findings = project_findings(prepared, &Fields::focus(), summary);
    let sections: Vec<Value> = SECTION_IDS
        .iter()
        .map(|id| json!({"id": id, "status": "in-progress"}))
        .collect();
    let images: Vec<Value> = prepared
        .images
        .iter()
        .map(|i| json!({"name": i.name}))
        .collect();
    json!({
        "format": "projects/v2",
        "id": project_id(prepared),
        "name": data["title"],
        "language": options.language,
        "tags": ["focus"],
        "override_finding_order": true,
        "report_data": data,
        "members": [],
        "project_type": design::project_type(&options.language),
        "sections": sections,
        "findings": findings,
        "images": images,
        "files": [],
        "comments": [],
    })
}
