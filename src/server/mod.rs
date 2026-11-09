//! MCP server: wiring, static resources and resource templates.

mod prompts;
mod tools;

use std::fmt::Write as _;

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::prompt::PromptRouter, router::tool::ToolRouter},
    model::{
        Implementation, ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams,
        ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
        ResourceContents, ResourceTemplate, ServerCapabilities, ServerConfig,
    },
    service::RequestContext,
};
use serde_json::{Value, json};

use crate::client::DatajudClient;
use crate::config::DEFAULT_BASE_URL;
use crate::{glossary, tribunals};

/// The DATAJUD MCP server: tools, prompts and resources over the public API.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DatajudServer {
    /// API client shared by all tools and prompts.
    client: DatajudClient,
    /// Router for the MCP tool handlers.
    tool_router: ToolRouter<Self>,
    /// Router for the MCP prompt handlers.
    prompt_router: PromptRouter<Self>,
}

impl DatajudServer {
    /// Builds the server around an API client.
    #[must_use]
    #[inline]
    pub fn new(client: DatajudClient) -> Self {
        Self {
            client,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }
}

// ---------------------------------------------------------------------------
// Static resources
// ---------------------------------------------------------------------------

/// URI of the tribunal catalog resource.
pub const URI_TRIBUNALS: &str = "datajud://tribunals";
/// URI of the data glossary resource.
pub const URI_GLOSSARY: &str = "datajud://glossary";
/// URI of the query examples resource.
pub const URI_EXAMPLES: &str = "datajud://examples";
/// URI of the terms-of-use resource.
pub const URI_TERMS: &str = "datajud://terms-of-use";

/// Examples document — mirrors the official wiki examples plus tool mappings.
#[must_use]
#[inline]
pub fn examples_markdown() -> String {
    r#"# Query examples — DATAJUD public API

Official sources: https://datajud-wiki.cnj.jus.br/api-publica/exemplos/

API request fields keep their PT-BR names (`numeroProcesso`, `classe.codigo`, ...).

## Ex. 1 — Search by case number (TRF1)

```json
{
  "query": {
    "match": {
      "numeroProcesso": "00008323520184013202"
    }
  }
}
```

Equivalent with this server's tools:
`get_case(tribunal="trf1", process_number="00008323520184013202")`
or `search_cases(tribunal="trf1", filters={"process_number": "00008323520184013202"})`.

## Ex. 2 — Procedural class + judging body (TJDFT)

```json
{
  "query": {
    "bool": {
      "must": [
        {"match": {"classe.codigo": 1116}},
        {"match": {"orgaoJulgador.codigo": 13597}}
      ]
    }
  }
}
```

Equivalent: `search_cases(tribunal="tjdft", filters={"class_code": 1116, "judging_body_code": 13597})`.

## Ex. 3 — Pagination with `search_after`

First page (100 records, sorted by `@timestamp`):

```json
{
  "size": 100,
  "query": { "bool": { "must": [ {"match": {"classe.codigo": 1116}}, {"match": {"orgaoJulgador.codigo": 13597}} ] } },
  "sort": [ { "@timestamp": { "order": "asc" } } ]
}
```

The response carries a `sort` array per hit (e.g. `[1681366085550]`). For the next
page, pass that array as `search_after`:

```json
{
  "size": 100,
  "query": { "bool": { "must": [ {"match": {"classe.codigo": 1116}}, {"match": {"orgaoJulgador.codigo": 13597}} ] } },
  "sort": [ { "@timestamp": { "order": "asc" } } ],
  "search_after": [ 1681366085550 ]
}
```

With this server's tools: `search_cases(..., size=100, search_after=[1681366085550])`
— the response itself returns `next_page` when more results exist.

## Aggregation (documented DSL)

```json
{
  "size": 0,
  "query": { "range": { "dataAjuizamento": { "gte": "20240101000000", "lte": "20241231235959" } } },
  "aggs": { "by_class": { "terms": { "field": "classe.codigo", "size": 10 } } }
}
```

Equivalent: `aggregate_cases(tribunal=..., field="classe.codigo", kind="terms")`.
Or send the arbitrary body with `search_dsl`.

## Escape hatch

Any search body in the documented DSL can be sent with
`search_dsl(tribunal="tjsp", body={...})` — the server forwards the
`POST /api_publica_{alias}/_search` request unchanged."#
        .to_owned()
}

/// Terms-of-use summary — consuming the API implies acceptance.
#[must_use]
#[inline]
pub fn terms_of_use_markdown() -> String {
    "# Terms of use — DATAJUD public API

Official document (PDF): https://formularios.cnj.jus.br/wp-content/uploads/2023/11/Termos-de-uso-api-publica-V1.2.pdf
Wiki: https://datajud-wiki.cnj.jus.br/api-publica/termo-uso

**Warning:** consuming the API implies full acceptance of the terms.
Points emphasized by the wiki:

1.2 The user tacitly accepts the conditions of the term by using the interface;
3.2 The user is responsible for the use of the interface and derived information;
3.3 The API is provided exclusively for legal, non-commercial and authorized purposes — misuse, abusive, illegal, malicious or immoral use is strictly prohibited;
3.6 The CNJ does not guarantee the accuracy, integrity or currency of the data;
3.8 The user agrees not to modify, distribute, sell or commercially exploit the API or derived information;
3.9 The user agrees to notify the CNJ of any information, news, study, report or document made available to the general public derived from the API.

Data comes from the National Database of the Judiciary (Datajud), protecting
classified cases and party data (CNJ Ordinance 160/2020).
"
    .to_owned()
}

/// Renders the full tribunal catalog as Markdown (the `datajud://tribunals` resource).
#[must_use]
#[inline]
pub fn tribunals_markdown() -> String {
    let mut out = String::from(
        "# Tribunals — DATAJUD public API\n\n\
         91 tribunal indices (`api_publica_<alias>`), per \
         https://datajud-wiki.cnj.jus.br/api-publica/endpoints\n\n\
         Search URL: `https://api-publica.datajud.cnj.jus.br/api_publica_<alias>/_search`\n\n",
    );
    let mut current_segment = "";
    for tribunal in tribunals::TRIBUNALS {
        let segment = tribunal.segment.label();
        if segment != current_segment {
            writeln!(out, "\n## {segment}\n\n| Alias | Tribunal |\n|---|---|").unwrap_or(());
            current_segment = segment;
        }
        writeln!(out, "| `{}` | {} |", tribunal.alias, tribunal.name).unwrap_or(());
    }
    out
}

/// JSON details for one tribunal (the `datajud://tribunals/{alias}` template).
fn tribunal_resource_json(alias: &str) -> Option<Value> {
    let tribunal = tribunals::find(alias)?;
    Some(json!({
        "alias": tribunal.alias,
        "name": tribunal.name,
        "segment": tribunal.segment.as_str(),
        "index": tribunal.index_name(),
        "search_url": tribunal.search_url(DEFAULT_BASE_URL),
        "count_url": tribunal.count_url(DEFAULT_BASE_URL),
    }))
}

// ---------------------------------------------------------------------------
// ServerHandler: info + resources (tools/prompts are wired by the macros)
// ---------------------------------------------------------------------------

#[expect(
    clippy::unused_async_trait_impl,
    reason = "the MCP ServerHandler trait contract requires async methods even without awaits"
)]
#[rmcp::tool_handler(router = "self.tool_router")]
#[rmcp::prompt_handler(router = "self.prompt_router")]
impl ServerHandler for DatajudServer {
    #[inline]
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new(
            "datajud-mcp",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "DATAJUD public API (CNJ) — metadata of Brazilian judiciary cases (capas processuais \
             and movimentos) from all 91 tribunals. Queries use the DSL documented by the CNJ (Elasticsearch-compatible syntax).\n\
             Start with list_tribunals (aliases) and list_fields (queryable attributes; the API's \
             PT-BR field names such as numeroProcesso, classe.codigo, orgaoJulgador.codigo). Use \
             search_cases/get_case/count_cases/aggregate_cases for structured access and \
             search_dsl for arbitrary query bodies in that DSL. Deep pagination: size <= 10000 and \
             search_after sorted by @timestamp. Queries on large tribunals can take 30+ seconds.\n\
             IMPORTANT: consuming this API implies accepting the CNJ terms of use \
             (datajud://terms-of-use): legal, non-commercial use only; cite the CNJ/Datajud in \
             derived public reports."
                .to_owned(),
        )
    }

    #[inline]
    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new(URI_TRIBUNALS, "Tribunals (full catalog)")
                .with_description("Catalog of all 91 tribunal aliases, names and index URLs")
                .with_mime_type("text/markdown"),
            Resource::new(URI_GLOSSARY, "Data glossary")
                .with_description("Field glossary of the case metadata schema")
                .with_mime_type("text/markdown"),
            Resource::new(URI_EXAMPLES, "Query examples")
                .with_description(
                    "Official example queries (case number, class+court, search_after pagination)",
                )
                .with_mime_type("text/markdown"),
            Resource::new(URI_TERMS, "Terms of use")
                .with_description("Terms of use summary — acceptance is implied by API consumption")
                .with_mime_type("text/markdown"),
        ]))
    }

    #[inline]
    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new("datajud://tribunals/{alias}", "Tribunal details")
                .with_description("Per-tribunal details; {alias} is an index alias such as 'tjsp'"),
        ]))
    }

    #[inline]
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        let uri = request.uri.as_str();
        let contents =
            match uri {
                URI_TRIBUNALS => ResourceContents::text(tribunals_markdown(), uri)
                    .with_mime_type("text/markdown"),
                URI_GLOSSARY => ResourceContents::text(glossary::render_markdown(), uri)
                    .with_mime_type("text/markdown"),
                URI_EXAMPLES => {
                    ResourceContents::text(examples_markdown(), uri).with_mime_type("text/markdown")
                }
                URI_TERMS => ResourceContents::text(terms_of_use_markdown(), uri)
                    .with_mime_type("text/markdown"),
                other if other.starts_with("datajud://tribunals/") => {
                    let alias = other.trim_start_matches("datajud://tribunals/");
                    match tribunal_resource_json(alias) {
                        Some(details) => ResourceContents::text(details.to_string(), other)
                            .with_mime_type("application/json"),
                        None => {
                            return Err(McpError::resource_not_found(
                                "unknown_tribunal_alias",
                                Some(json!({ "uri": other, "hint": "see datajud://tribunals" })),
                            ));
                        }
                    }
                }
                other => {
                    return Err(McpError::resource_not_found(
                        "resource_not_found",
                        Some(json!({ "uri": other })),
                    ));
                }
            };
        Ok(ReadResourceResult::new(vec![contents]).into())
    }
}
