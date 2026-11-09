//! MCP prompts: reusable workflows for querying and analyzing DATAJUD.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{GetPromptResult, PromptMessage, Role},
    prompt, prompt_router,
};

use super::DatajudServer;

/// Default hint when no tribunal was given to `build_query`.
fn default_tribunal_hint() -> String {
    "(to be defined — see list_tribunals)".to_owned()
}

/// Arguments for the `build_query` prompt.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct BuildQueryArgs {
    /// Research goal in natural language (e.g. "volume of labor cases filed in 2024 in São Paulo").
    goal: String,
    /// Tribunal alias(es) to target (e.g. "tjsp" or "trf1,trf2"); defaults to a hint.
    #[serde(default = "default_tribunal_hint")]
    tribunal: String,
}

/// Arguments for the `analyze_case` prompt.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct AnalyzeCaseArgs {
    /// Tribunal alias, e.g. "tjsp".
    tribunal: String,
    /// CNJ unified case number without formatting (API field `numeroProcesso`).
    process_number: String,
    /// Analytical focus (e.g. "timeline of movements"); defaults to the full timeline.
    #[serde(default = "default_focus")]
    focus: String,
}

/// Default analytical focus for `analyze_case`.
fn default_focus() -> String {
    "full timeline and current status".to_owned()
}

/// Arguments for the `research_report` prompt.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ResearchReportArgs {
    /// Topic of the report (e.g. "tax executions filed in 2023-2024").
    topic: String,
    /// Tribunal aliases to cover (e.g. "trf1,trf2,trf3"); defaults to a hint.
    #[serde(default = "default_tribunals_hint")]
    tribunals: String,
}

/// Default hint when no tribunals were given to `research_report`.
fn default_tribunals_hint() -> String {
    "(select via list_tribunals)".to_owned()
}

#[expect(
    clippy::unused_self,
    missing_docs,
    reason = "MCP prompt handlers are instance methods by macro contract; the rmcp #[prompt] macro consumes doc comments during expansion"
)]
#[prompt_router(vis = "pub(crate)")]
impl DatajudServer {
    /// Guides building a well-formed DATAJUD query for a research goal.
    #[prompt(
        name = "build_query",
        description = "Design a DATAJUD query plan from a natural-language research goal: choose tools, filters, aggregation and pagination strategy."
    )]
    fn build_query(&self, Parameters(args): Parameters<BuildQueryArgs>) -> GetPromptResult {
        let tribunal = args.tribunal;
        GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "I need to query the DATAJUD public API (CNJ).\n\n\
                 Goal: {goal}\n\
                 Tribunals: {tribunal}\n\n\
                 Design a step-by-step query plan:\n\
                 1. Use `list_tribunals` to confirm the correct tribunal aliases;\n\
                 2. Use `list_fields` (or the datajud://glossary resource) to pick the right attributes — API field names are PT-BR (numeroProcesso, classe.codigo, orgaoJulgador.codigo, assuntos.codigo, movimentos.codigo, dataAjuizamento, grau, ...);\n\
                 3. Prefer `count_cases` to size the population before searching;\n\
                 4. For distributions/statistics use `aggregate_cases` (terms or date_histogram); for listings use `search_cases` with `filters` and pagination via `search_after` sorted by @timestamp;\n\
                 5. If needed, fall back to the `search_dsl` escape hatch with a full query body in the documented DSL (see datajud://examples);\n\
                 6. Dates: dataAjuizamento accepts YYYY-MM-DD (converted to AAAAMMDDHHMMSS); movement/update dates pass through as ISO-8601.\n\n\
                 The API can take 30+ seconds on large tribunals; keep multi-tribunal requests to at most 6 aliases.\
                 Finally, cite the CNJ/Datajud source and remember use is governed by the terms of use (datajud://terms-of-use):\
                 legal, non-commercial purposes only, and public reports must notify the CNJ.\n\n\
                 Detailed goal: {goal}",
                goal = args.goal,
                tribunal = tribunal,
            ),
        )])
        .with_description("DATAJUD query plan")
    }

    /// Analyzes one case: fetch + interpret the movimentos timeline.
    #[prompt(
        name = "analyze_case",
        description = "Fetch a case by CNJ number and analyze its metadata and movement timeline (capa processual + movimentos)."
    )]
    fn analyze_case(&self, Parameters(args): Parameters<AnalyzeCaseArgs>) -> GetPromptResult {
        let focus = args.focus;
        GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Analyze case {process_number} in tribunal {tribunal} using the DATAJUD public API.\n\n\
                 1. Call `get_case(tribunal=\"{tribunal}\", process_number=\"{process_number}\", include_movements=true)`;\n\
                 2. If not found, check the unformatted 20-digit number and the correct tribunal — classified cases (sigilosos) are not exposed;\n\
                 3. Report: class (classe), subjects (assuntos), judging body (orgaoJulgador, with IBGE municipality), instance (grau), system/format, filing date (dataAjuizamento) and secrecy level (nivelSigilo);\n\
                 4. Build the `movimentos` timeline (codigo, nome, dataHora) and highlight relevant complementosTabelados;\n\
                 5. Interpret the movements in plain legal language.\n\n\
                 Analysis focus: {focus}\n\
                 Source to cite: CNJ/Datajud (public API) — terms of use at datajud://terms-of-use.",
                process_number = args.process_number,
                tribunal = args.tribunal,
                focus = focus,
            ),
        )])
        .with_description("DATAJUD case analysis")
    }

    /// Builds a research report that honors the terms of use.
    #[prompt(
        name = "research_report",
        description = "Produce a Datajud-based research report with counts, aggregations and citations — including the CNJ terms-of-use obligations for public reports."
    )]
    fn research_report(&self, Parameters(args): Parameters<ResearchReportArgs>) -> GetPromptResult {
        let tribunals = args.tribunals;
        GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Write a research report about: {topic}\n\
                 Tribunals: {tribunals}\n\n\
                 Method:\n\
                 1. Confirm aliases (`list_tribunals`) and fields (`list_fields`);\n\
                 2. Size the population with `count_cases`; detail with `aggregate_cases` (e.g. by classe.codigo, orgaoJulgador.codigo, dataAjuizamento histogram per year);\n\
                 3. Illustrate with a few representative cases via `search_cases` (small size, without movements);\n\
                 4. Structure: context, methodology, quantitative results, limitations (case-header data only, no party data; the CNJ does not guarantee currency — item 3.6 of the terms), conclusions;\n\
                 5. Always cite the source: CNJ — Datajud, public API (https://datajud-wiki.cnj.jus.br/api-publica/).\n\n\
                 Terms of use (datajud://terms-of-use): legal, non-commercial use;\
                 if this report is published, notify the CNJ (item 3.9);\
                 do not commercialize the data or derived works (item 3.8).",
                topic = args.topic,
                tribunals = tribunals,
            ),
        )])
        .with_description("DATAJUD research report")
    }
}
