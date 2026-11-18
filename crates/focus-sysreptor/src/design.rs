use serde_json::{Value, json};

use crate::ids::uuid;

pub(crate) const DESIGN_NAME: &str = "Focus engagement v1";
const DESIGN_VERSION: &str = "1";

pub(crate) fn design_id() -> String {
    uuid(&["focus-sysreptor-design", DESIGN_VERSION])
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fields {
    pub title: String,
    pub cvss: String,
    pub severity: Option<String>,
    pub cwe: Option<String>,
    pub affected_components: Option<String>,
    pub description: String,
    pub evidence: Option<String>,
    pub impact: Option<String>,
    pub recommendation: Option<String>,
    pub references: Option<String>,
    pub attack: Option<String>,
    pub status: Option<String>,
}

impl Fields {
    pub fn focus() -> Fields {
        let some = |id: &str| Some(id.to_string());
        Fields {
            title: "title".into(),
            cvss: "cvss".into(),
            severity: some("severity"),
            cwe: some("cwe"),
            affected_components: some("affected_components"),
            description: "description".into(),
            evidence: some("evidence"),
            impact: some("impact"),
            recommendation: some("recommendation"),
            references: some("references"),
            attack: some("attack"),
            status: some("finding_status"),
        }
    }

    pub fn sysreptor_default() -> Fields {
        let some = |id: &str| Some(id.to_string());
        Fields {
            title: "title".into(),
            cvss: "cvss".into(),
            severity: None,
            cwe: None,
            affected_components: some("affected_components"),
            description: "description".into(),
            evidence: None,
            impact: some("impact"),
            recommendation: some("recommendation"),
            references: some("references"),
            attack: None,
            status: None,
        }
    }
}

pub(crate) const SECTION_IDS: [&str; 5] = ["customer", "scope", "hosts", "appendix", "other"];

fn string_field(
    id: &str,
    label: &str,
    origin: &str,
    required: bool,
    default: Option<&str>,
) -> Value {
    let mut field = json!({"id": id, "type": "string", "label": label, "origin": origin, "required": required, "spellcheck": false});
    if let Some(default) = default {
        field["default"] = json!(default);
    }
    field
}

fn markdown_field(id: &str, label: &str, required: bool) -> Value {
    json!({"id": id, "type": "markdown", "label": label, "origin": "custom", "required": required})
}

fn date_field(id: &str, label: &str) -> Value {
    json!({"id": id, "type": "date", "label": label, "origin": "custom", "required": false})
}

fn enum_field(id: &str, label: &str, origin: &str, choices: &[(&str, &str)]) -> Value {
    let choices: Vec<Value> = choices
        .iter()
        .map(|(value, label)| json!({"value": value, "label": label}))
        .collect();
    json!({"id": id, "type": "enum", "label": label, "origin": origin, "required": false, "choices": choices})
}

fn list_of_strings(id: &str, label: &str, origin: &str, item_label: &str) -> Value {
    json!({
        "id": id, "type": "list", "label": label, "origin": origin, "required": false,
        "items": {"id": "", "type": "string", "label": item_label, "origin": origin, "required": true, "spellcheck": false}
    })
}

pub(crate) fn finding_fields() -> Value {
    json!([
        {"id": "title", "type": "string", "label": "Title", "origin": "core", "required": true, "spellcheck": true, "default": "TODO: finding title"},
        {"id": "cvss", "type": "cvss", "label": "CVSS", "origin": "predefined", "required": true, "default": "n/a"},
        enum_field("severity", "Severity", "predefined", &[("info", "Info"), ("low", "Low"), ("medium", "Medium"), ("high", "High"), ("critical", "Critical")]),
        {"id": "cwe", "type": "cwe", "label": "CWE", "origin": "custom", "required": false},
        list_of_strings("affected_components", "Affected components", "predefined", "Component"),
        markdown_field("description", "Description", true),
        markdown_field("evidence", "Evidence", false),
        markdown_field("impact", "Impact", false),
        markdown_field("recommendation", "Recommendation", false),
        list_of_strings("references", "References", "predefined", "Reference"),
        list_of_strings("attack", "MITRE ATT&CK techniques", "custom", "Technique"),
        enum_field("finding_status", "Focus status", "custom", &[("draft", "Draft"), ("confirmed", "Confirmed"), ("reported", "Reported"), ("fixed", "Fixed"), ("accepted", "Accepted")]),
    ])
}

pub(crate) fn finding_ordering() -> Value {
    json!([{"field": "cvss", "order": "desc"}, {"field": "title", "order": "asc"}])
}

pub(crate) fn report_sections() -> Value {
    json!([
        {"id": "customer", "label": "Customer", "fields": [string_field("customer", "Customer", "custom", false, None)]},
        {"id": "scope", "label": "Scope", "fields": [
            markdown_field("scope", "Scope", false),
            date_field("start_date", "Start date"),
            date_field("end_date", "End date"),
        ]},
        {"id": "hosts", "label": "Hosts", "fields": [markdown_field("hosts", "Hosts and services", false)]},
        {"id": "appendix", "label": "Appendix", "fields": [markdown_field("credentials", "Credentials", false)]},
        {"id": "other", "label": "Other", "fields": [
            {"id": "title", "type": "string", "label": "Title", "origin": "core", "required": true, "spellcheck": true, "default": "TODO: report title"},
            date_field("report_date", "Report date"),
            string_field("author", "Author", "custom", false, None),
            string_field("engagement_type", "Engagement type", "custom", false, None),
            string_field("engagement_status", "Engagement status", "custom", false, None),
        ]},
    ])
}

const TEMPLATE: &str = r#"<section id="page-title">
  <h1>{{ report.title }}</h1>
  <p v-if="report.customer">{{ report.customer }}</p>
  <p v-if="report.engagement_type">{{ report.engagement_type }}</p>
  <p v-if="report.report_date">{{ formatDate(report.report_date, 'long', 'en-US') }}</p>
  <p v-if="report.author">{{ report.author }}</p>
  <pagebreak />
</section>

<section id="scope" v-if="report.scope">
  <h1 class="in-toc numbered">Scope</h1>
  <markdown :text="report.scope" />
</section>

<section id="hosts" v-if="report.hosts">
  <h1 class="in-toc numbered">Hosts and services</h1>
  <markdown :text="report.hosts" />
</section>

<section id="findings-list">
  <h1 class="in-toc numbered">Findings</h1>
  <div v-for="finding in findings">
    <h2 :id="finding.id" class="in-toc numbered">{{ finding.title }}</h2>
    <p>
      <strong v-if="finding.severity && finding.severity.label">Severity: {{ finding.severity.label }}<br></strong>
      <template v-if="finding.cvss && finding.cvss.vector && finding.cvss.vector.startsWith('CVSS:')">
        <strong>CVSS: {{ finding.cvss.score }}</strong> {{ finding.cvss.vector }}<br>
      </template>
      <template v-if="finding.cwe && finding.cwe.value">CWE: {{ finding.cwe.value }}<br></template>
      <template v-if="finding.affected_components && finding.affected_components.length > 0">
        Affected: <span v-for="(component, index) in finding.affected_components">{{ component }}<span v-if="index < finding.affected_components.length - 1">, </span></span>
      </template>
    </p>
    <markdown :text="finding.description" />
    <template v-if="finding.evidence">
      <h3>Evidence</h3>
      <markdown :text="finding.evidence" />
    </template>
    <template v-if="finding.impact">
      <h3>Impact</h3>
      <markdown :text="finding.impact" />
    </template>
    <template v-if="finding.recommendation">
      <h3>Recommendation</h3>
      <markdown :text="finding.recommendation" />
    </template>
    <template v-if="finding.references && finding.references.length > 0">
      <h3>References</h3>
      <ul><li v-for="reference in finding.references">{{ reference }}</li></ul>
    </template>
    <pagebreak />
  </div>
</section>

<section id="appendix" v-if="report.credentials">
  <h1 class="in-toc numbered">Credentials</h1>
  <markdown :text="report.credentials" />
</section>
"#;

const STYLES: &str = r#"@import "/assets/global/base.css";

html {
  font-size: 11pt;
}

table {
  border-collapse: collapse;
}

th, td {
  border: 1px solid #999;
  padding: 0.2em 0.5em;
}
"#;

pub(crate) fn project_type(language: &str) -> Value {
    json!({
        "format": "projecttypes/v2",
        "id": design_id(),
        "name": DESIGN_NAME,
        "language": language,
        "status": "finished",
        "tags": ["focus"],
        "report_sections": report_sections(),
        "finding_fields": finding_fields(),
        "finding_ordering": finding_ordering(),
        "report_template": TEMPLATE,
        "report_styles": STYLES,
        "report_preview_data": {},
        "assets": [],
    })
}
