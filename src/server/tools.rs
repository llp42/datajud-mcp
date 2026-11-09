//! MCP tools exposing the full DATAJUD public API surface.
//!
//! Coverage:
//! * `list_tribunals`   — the 91 tribunal indices.
//! * `list_fields`      — queryable attribute glossary.
//! * `search_cases`     — structured search (bool query) with pagination.
//! * `get_case`         — single case by CNJ number or Datajud id.
//! * `count_cases`      — document counts (`POST _count`).
//! * `aggregate_cases`  — terms / date-histogram aggregations.
//! * `search_dsl`       — raw query DSL passthrough (100% escape hatch).
//! * `test_connection`  — liveness + latency probe.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    tool, tool_router,
};
use serde_json::{Value, json};

use super::DatajudServer;
use crate::error::DatajudError;
use crate::query::{AggKind, SearchFilters, SortField, SortOrder};
use crate::{glossary, query, tribunals};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Wraps a JSON value as a pretty-printed text content block.
fn json_text(value: &Value) -> Vec<ContentBlock> {
    vec![ContentBlock::text(
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
    )]
}

/// Wraps a JSON value as a successful tool result.
fn ok_json(value: &Value) -> CallToolResult {
    CallToolResult::success(json_text(value))
}

/// Wraps an error (with hint) as a tool-level error result.
fn fail(error: &DatajudError) -> CallToolResult {
    let mut payload = serde_json::Map::new();
    payload.insert("error".to_owned(), json!(error.to_string()));
    if let Some(hint) = error.hint() {
        payload.insert("hint".to_owned(), json!(hint));
    }
    CallToolResult::error(json_text(&Value::Object(payload)))
}

/// Splits the `tribunal` parameter into validated aliases (max 6 per request,
/// to keep consumption of the public API polite — see the terms of use).
fn resolve_tribunals(spec: &str) -> Result<Vec<&'static tribunals::Tribunal>, DatajudError> {
    let mut resolved = Vec::new();
    for part in spec
        .split(',')
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
    {
        match tribunals::find(part) {
            Some(tribunal) => {
                if !resolved
                    .iter()
                    .any(|other: &&tribunals::Tribunal| other.alias == tribunal.alias)
                {
                    resolved.push(tribunal);
                }
            }
            None => {
                return Err(DatajudError::UnknownTribunal {
                    alias: part.to_owned(),
                });
            }
        }
    }
    if resolved.is_empty() {
        return Err(DatajudError::InvalidInput {
            message: "parameter 'tribunal' is required (e.g. 'tjsp' or 'trf1,trf2')".to_owned(),
        });
    }
    if resolved.len() > 6 {
        return Err(DatajudError::InvalidInput {
            message: format!(
                "at most 6 tribunals per request (got {}); the terms of use prohibit abusive consumption",
                resolved.len()
            ),
        });
    }
    Ok(resolved)
}

/// Resolves exactly one tribunal alias (errors on lists of length != 1).
fn resolve_one_tribunal(spec: &str) -> Result<&'static tribunals::Tribunal, DatajudError> {
    let mut resolved = resolve_tribunals(spec)?;
    if resolved.len() != 1 {
        return Err(DatajudError::InvalidInput {
            message: format!(
                "this operation requires exactly one tribunal alias, got {} ('{}')",
                resolved.len(),
                spec
            ),
        });
    }
    Ok(resolved.remove(0))
}

/// Compact rendering of a hit: keeps `_source` (possibly trimmed) + sort cursor.
fn render_hit(hit: &Value) -> Value {
    let source = hit.get("_source").cloned().unwrap_or(Value::Null);
    json!({
        "id": hit.get("_id"),
        "score": hit.get("_score"),
        "sort": hit.get("sort"),
        "data": source,
    })
}

/// Extracts the `hits.hits` array from a raw search response.
fn hits_of(response: &Value) -> Vec<Value> {
    response
        .get("hits")
        .and_then(|node| node.get("hits"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// parameters (all names in English; PT-BR only for the API request fields)
// ---------------------------------------------------------------------------

/// Parameters for `list_tribunals`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ListTribunalsParams {
    /// Optional segment filter: superior | federal | state | labor | electoral | military.
    segment: Option<String>,
    /// Optional free text matched against alias and tribunal name.
    text: Option<String>,
}

/// Parameters for `list_fields`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ListFieldsParams {
    /// Optional free text matched against field name and description.
    text: Option<String>,
}

/// Parameters for `search_cases`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SearchCasesParams {
    /// Tribunal alias (e.g. `tjsp`) or a comma-separated list of up to 6 aliases (e.g. `trf1,trf2`).
    tribunal: String,
    /// Structured filters combined with AND semantics.
    filters: Option<SearchFilters>,
    /// Page size, 0..10000 (default 10).
    size: Option<u64>,
    /// Sort field for pagination: `timestamp` (@timestamp, default) or `filing_date` (dataAjuizamento).
    sort_field: Option<SortField>,
    /// Sort order: `asc` (default) or `desc`.
    sort_order: Option<SortOrder>,
    /// Pagination cursor: the `sort` array of the last hit of the previous page.
    search_after: Option<Value>,
    /// Include the (potentially large) movimentos array in each hit (default false).
    include_movements: Option<bool>,
    /// Restrict returned fields (API `_source` filter), e.g. [`numeroProcesso`,`classe`,`orgaoJulgador`].
    fields: Option<Vec<String>>,
}

/// Parameters for `get_case`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GetCaseParams {
    /// Tribunal alias, e.g. `tjsp`.
    tribunal: String,
    /// CNJ unified case number without formatting (API field `numeroProcesso`), e.g. `00008323520184013202`.
    process_number: Option<String>,
    /// Datajud document id (API field `id`), e.g. `TRF1_JE_00008323520184013202`. Alternative to `process_number`.
    id: Option<String>,
    /// Include the movimentos array (default true).
    include_movements: Option<bool>,
}

/// Parameters for `count_cases`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct CountCasesParams {
    /// Tribunal alias or a comma-separated list of up to 6 aliases.
    tribunal: String,
    /// Structured filters combined with AND semantics (same as `search_cases`).
    filters: Option<SearchFilters>,
}

/// Parameters for `aggregate_cases`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct AggregateCasesParams {
    /// Tribunal alias (single index), e.g. `tjmg`.
    tribunal: String,
    /// Filters to restrict the aggregation population.
    filters: Option<SearchFilters>,
    /// Aggregation kind: `terms` (top values) or `date_histogram` (date histogram).
    kind: AggKind,
    /// Field to aggregate (terms): one of the aggregable fields — see `list_fields`.
    field: String,
    /// Max buckets for terms aggregations (default 20).
    bucket_size: Option<u64>,
    /// Date field for `date_histogram`: `dataAjuizamento` (default) | `dataHoraUltimaAtualizacao` | `@timestamp` | `movimentos.dataHora`.
    date_field: Option<String>,
    /// Calendar interval for `date_histogram`: `day` | `week` | `month` | `quarter` | `year` (default year).
    interval: Option<String>,
}

/// Parameters for `search_dsl`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct SearchDslParams {
    /// Tribunal alias (single index), e.g. `tjsp`.
    tribunal: String,
    /// Full search body in the documented DSL (`query`, `size`, `from`, `sort`, `search_after`, `aggs`, `_source`, ...).
    body: Value,
}

/// Parameters for `test_connection`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct TestConnectionParams {
    /// Tribunal alias to probe (default `trf1`).
    tribunal: Option<String>,
}

// ---------------------------------------------------------------------------
// tool implementations
// ---------------------------------------------------------------------------

#[expect(
    clippy::unused_self,
    reason = "MCP tool handlers are instance methods by macro contract"
)]
#[tool_router(vis = "pub(crate)")]
impl DatajudServer {
    /// Lists the 91 tribunal indices, optionally filtered by segment or text.
    #[tool(
        name = "list_tribunals",
        description = "List the 91 tribunal indices of the DATAJUD public API (aliases like 'tjsp', 'trf1', 'tre-sp', 'tst'). Each alias maps to the index 'api_publica_<alias>'. Optionally filter by segment (superior|federal|state|labor|electoral|military) or free text."
    )]
    fn list_tribunals(
        &self,
        Parameters(params): Parameters<ListTribunalsParams>,
    ) -> CallToolResult {
        let segment = match params.segment.as_deref() {
            None => None,
            Some(raw) => match tribunals::Segment::parse(raw) {
                Some(parsed) => Some(parsed),
                None => {
                    return fail(&DatajudError::InvalidInput {
                        message: format!(
                            "unknown segment '{raw}'; use superior, federal, state, labor, electoral or military"
                        ),
                    });
                }
            },
        };
        let found = tribunals::search(segment, params.text.as_deref());
        let items: Vec<Value> = found
            .iter()
            .map(|tribunal| {
                json!({
                    "alias": tribunal.alias,
                    "name": tribunal.name,
                    "segment": tribunal.segment.as_str(),
                    "index": tribunal.index_name(),
                    "search_url": tribunal.search_url(&self.client.config().base_url),
                })
            })
            .collect();
        ok_json(&json!({
            "total": items.len(),
            "available": tribunals::total(),
            "tribunals": items,
        }))
    }

    /// Glossary of the queryable case-metadata attributes.
    #[tool(
        name = "list_fields",
        description = "Glossary of the queryable case-metadata attributes of the DATAJUD API (PT-BR API field names: numeroProcesso, dataAjuizamento, classe.codigo, orgaoJulgador.codigo, movimentos.codigo, @timestamp, ...). Returns field paths, field types (as in the official glossary) and descriptions, plus date-format and pagination notes."
    )]
    fn list_fields(&self, Parameters(params): Parameters<ListFieldsParams>) -> CallToolResult {
        let fields: Vec<Value> = glossary::search(params.text.as_deref())
            .iter()
            .map(|field| {
                json!({
                    "field": field.name,
                    "field_type": field.field_type,
                    "description": field.description,
                })
            })
            .collect();
        ok_json(&json!({
            "total": fields.len(),
            "fields": fields,
            "aggregatable_fields": glossary::AGGREGATABLE_FIELDS,
            "filed_date_format": "dataAjuizamento is stored as AAAAMMDDHHMMSS (e.g. 20181029000000); the tools accept YYYY-MM-DD and convert it",
            "pagination": "size <= 10000 per page; use sort on @timestamp + search_after for subsequent pages",
            "resource": "the datajud://glossary resource has the full glossary in Markdown",
        }))
    }

    /// Structured search with filters, sorting and `search_after` pagination.
    #[tool(
        name = "search_cases",
        description = "Search case metadata (capas processuais) on one or more DATAJUD tribunal indices. Filters (process_number, class_code, judging_body_code, subject_code, instance, filing_date_from/until, movement_code, free_text, ...) are combined with AND. Returns hits with the case metadata plus 'next_page' (search_after cursor) when more results exist. Page size defaults to 10 (max 10000); deep pagination must use search_after sorted by @timestamp. Movements (movimentos) are excluded by default to keep payloads small."
    )]
    async fn search_cases(
        &self,
        Parameters(params): Parameters<SearchCasesParams>,
    ) -> CallToolResult {
        let chosen = match resolve_tribunals(&params.tribunal) {
            Ok(resolved) => resolved,
            Err(error) => return fail(&error),
        };
        let filters = params.filters.unwrap_or_default();
        let size = params.size.unwrap_or(query::DEFAULT_SIZE);
        let sort_field = params.sort_field.unwrap_or_default();
        let sort_order = params.sort_order.unwrap_or_default();
        let include_movements = params.include_movements.unwrap_or(false);

        let body = match query::build_search_body(
            &filters,
            size,
            sort_field,
            sort_order,
            params.search_after.as_ref(),
            params.fields.as_deref(),
            include_movements,
        ) {
            Ok(built) => built,
            Err(error) => return fail(&error),
        };

        // Single tribunal: pass the raw response through (full fidelity).
        if chosen.len() == 1 {
            if let Some(&tribunal) = chosen.first() {
                return match self.client.search(&tribunal.index_name(), &body).await {
                    Ok(response) => {
                        let page_hits = hits_of(&response);
                        let last_sort = page_hits.last().and_then(|hit| hit.get("sort")).cloned();
                        let mut out = json!({
                            "tribunal": tribunal.alias,
                            "index": tribunal.index_name(),
                            "took_ms": response.get("took"),
                            "timed_out": response.get("timed_out"),
                            "shards": response.get("_shards"),
                            "total": response.get("hits").and_then(|node| node.get("total")),
                            "max_score": response.get("hits").and_then(|node| node.get("max_score")),
                            "cases": page_hits.iter().map(render_hit).collect::<Vec<_>>(),
                        });
                        if let Some(map) = out.as_object_mut() {
                            if let Some(next) =
                                query::next_page_hint(last_sort.as_ref(), size, page_hits.len())
                            {
                                map.insert("next_page".to_owned(), next);
                            }
                            if response
                                .get("hits")
                                .and_then(|node| node.get("total"))
                                .and_then(|node| node.get("relation"))
                                == Some(&json!("gte"))
                            {
                                map.insert(
                                "warning".to_owned(),
                                json!("total is truncated at 10000 matches ('relation': 'gte'); refine filters or aggregate instead of scanning"),
                            );
                            }
                        }
                        ok_json(&out)
                    }
                    Err(error) => fail(&error),
                };
            }
        }

        // Multi-tribunal: merge compact results.
        let mut merged = Vec::new();
        let mut totals = Vec::new();
        let mut errors = Vec::new();
        for tribunal in &chosen {
            match self.client.search(&tribunal.index_name(), &body).await {
                Ok(response) => {
                    let page_hits = hits_of(&response);
                    totals.push(json!({
                        "tribunal": tribunal.alias,
                        "total": response.get("hits").and_then(|node| node.get("total")),
                        "returned": page_hits.len(),
                    }));
                    for hit in page_hits {
                        let mut rendered = render_hit(&hit);
                        if let Some(map) = rendered.as_object_mut() {
                            map.insert("tribunal".to_owned(), json!(tribunal.alias));
                        }
                        merged.push(rendered);
                    }
                }
                Err(error) => {
                    errors.push(json!({ "tribunal": tribunal.alias, "error": error.to_string() }));
                }
            }
        }
        return ok_json(&json!({
            "tribunals": chosen.iter().map(|tribunal| tribunal.alias).collect::<Vec<_>>(),
            "totals_by_tribunal": totals,
            "cases": merged,
            "warning": "multi-tribunal searches do not return 'next_page' — query each tribunal separately to paginate",
            "errors": errors,
        }));
    }

    /// Fetches one full case by CNJ number or Datajud id.
    #[tool(
        name = "get_case",
        description = "Fetch ONE case (capa processual) in full by its CNJ unified number (API field numeroProcesso, e.g. '00008323520184013202') or by its Datajud id (e.g. 'TRF1_JE_00008323520184013202'). Requires the tribunal alias. Returns the complete metadata including the movimentos timeline unless include_movements is false."
    )]
    async fn get_case(&self, Parameters(params): Parameters<GetCaseParams>) -> CallToolResult {
        let tribunal = match resolve_one_tribunal(&params.tribunal) {
            Ok(resolved) => resolved,
            Err(error) => return fail(&error),
        };
        let include_movements = params.include_movements.unwrap_or(true);

        let lookup = match (&params.id, &params.process_number) {
            (Some(id), _) => json!({ "term": { "id": id } }),
            (None, Some(number)) => json!({ "match": { "numeroProcesso": number } }),
            (None, None) => {
                return fail(&DatajudError::InvalidInput {
                    message: "provide 'process_number' or 'id'".to_owned(),
                });
            }
        };

        let mut body = serde_json::Map::new();
        body.insert("size".to_owned(), json!(1_u64));
        body.insert("query".to_owned(), lookup);
        if let Some(source) = query::build_source(None, include_movements) {
            body.insert("_source".to_owned(), source);
        }

        return match self
            .client
            .search(&tribunal.index_name(), &Value::Object(body))
            .await
        {
            Ok(response) => {
                let page_hits = hits_of(&response);
                page_hits.first().map_or_else(
                    || {
                        ok_json(&json!({
                            "tribunal": tribunal.alias,
                            "found": false,
                            "took_ms": response.get("took"),
                            "timed_out": response.get("timed_out"),
                            "shards": response.get("_shards"),
                            "hint": "check the unformatted 20-digit number and the correct tribunal; classified cases (sigilosos) are not exposed. If 'timed_out' is true or shards failed, retry — the public cluster is sometimes slow",
                        }))
                    },
                    |hit| {
                        ok_json(&json!({
                            "tribunal": tribunal.alias,
                            "found": true,
                            "took_ms": response.get("took"),
                            "timed_out": response.get("timed_out"),
                            "shards": response.get("_shards"),
                            "case": render_hit(hit),
                        }))
                    },
                )
            }
            Err(error) => fail(&error),
        };
    }

    /// Counts matching cases per tribunal (`POST _count`).
    #[tool(
        name = "count_cases",
        description = "Count case metadata matching filters on one or more tribunal indices (POST _count). Same filter semantics as search_cases but returns only counts — use it to size a population before searching or aggregating."
    )]
    async fn count_cases(
        &self,
        Parameters(params): Parameters<CountCasesParams>,
    ) -> CallToolResult {
        let chosen = match resolve_tribunals(&params.tribunal) {
            Ok(resolved) => resolved,
            Err(error) => return fail(&error),
        };
        let filters = params.filters.unwrap_or_default();
        let body = match query::build_count_body(&filters) {
            Ok(built) => built,
            Err(error) => return fail(&error),
        };

        let mut by_tribunal = Vec::new();
        let mut total: i64 = 0;
        let mut errors = Vec::new();
        for tribunal in &chosen {
            match self.client.count(&tribunal.index_name(), &body).await {
                Ok(response) => {
                    let count = response.get("count").and_then(Value::as_i64).unwrap_or(0);
                    total = total.saturating_add(count);
                    by_tribunal.push(json!({ "tribunal": tribunal.alias, "count": count }));
                }
                Err(error) => {
                    errors.push(json!({ "tribunal": tribunal.alias, "error": error.to_string() }));
                }
            }
        }
        return ok_json(&json!({
            "total": total,
            "by_tribunal": by_tribunal,
            "errors": errors,
        }));
    }

    /// Runs a terms or date-histogram aggregation over one tribunal index.
    #[tool(
        name = "aggregate_cases",
        description = "Run an aggregation over one tribunal index: 'terms' (top values of a field, e.g. classe.codigo, orgaoJulgador.codigo, movimentos.codigo) or 'date_histogram' (over dataAjuizamento / dataHoraUltimaAtualizacao / @timestamp / movimentos.dataHora with calendar_interval day|week|month|quarter|year). Filters restrict the population. Returns buckets with doc counts."
    )]
    async fn aggregate_cases(
        &self,
        Parameters(params): Parameters<AggregateCasesParams>,
    ) -> CallToolResult {
        let tribunal = match resolve_one_tribunal(&params.tribunal) {
            Ok(resolved) => resolved,
            Err(error) => return fail(&error),
        };
        let filters = params.filters.unwrap_or_default();
        let bucket_size = params.bucket_size.unwrap_or(20_u64).clamp(1_u64, 1000_u64);
        let body = match query::build_agg_body(
            &filters,
            &params.field,
            params.kind,
            bucket_size,
            params.interval.as_deref(),
            params.date_field.as_deref(),
        ) {
            Ok(built) => built,
            Err(error) => return fail(&error),
        };

        return match self.client.search(&tribunal.index_name(), &body).await {
            Ok(response) => {
                let aggregation = response
                    .get("aggregations")
                    .and_then(|node| node.get("aggregation"));
                ok_json(&json!({
                    "tribunal": tribunal.alias,
                    "kind": params.kind,
                    "field": params.field,
                    "total_documents": response.get("hits").and_then(|node| node.get("total")),
                    "took_ms": response.get("took"),
                    "buckets": aggregation
                        .and_then(|node| node.get("buckets"))
                        .cloned()
                        .unwrap_or(Value::Null),
                    "full_aggregation": aggregation.cloned().unwrap_or(Value::Null),
                }))
            }
            Err(error) => fail(&error),
        };
    }

    /// Raw query DSL passthrough — the full API escape hatch.
    #[tool(
        name = "search_dsl",
        description = "Raw query DSL passthrough (Elasticsearch-compatible syntax, as documented by the CNJ): POST the given body verbatim to 'https://api-publica.datajud.cnj.jus.br/api_publica_<tribunal>/_search'. Full API coverage — query, bool, range, movimentos filters, aggs, highlight, sort, search_after, _source, from/size, etc. Syntax reference: https://www.elastic.co/guide/en/elasticsearch/reference/current/search-your-data.html and the datajud://examples resource. Use this when the structured tools are not enough."
    )]
    async fn search_dsl(&self, Parameters(params): Parameters<SearchDslParams>) -> CallToolResult {
        let tribunal = match resolve_one_tribunal(&params.tribunal) {
            Ok(resolved) => resolved,
            Err(error) => return fail(&error),
        };
        if !params.body.is_object() {
            return fail(&DatajudError::InvalidInput {
                message: "'body' must be a JSON object (the search body)".to_owned(),
            });
        }
        return match self
            .client
            .search(&tribunal.index_name(), &params.body)
            .await
        {
            Ok(response) => ok_json(&response),
            Err(error) => fail(&error),
        };
    }

    /// Liveness + latency probe against one tribunal index.
    #[tool(
        name = "test_connection",
        description = "Probe the DATAJUD API: runs a size-0 match_all search on the given tribunal index (default 'trf1') and reports HTTP status, latency and total document count. Use to diagnose auth/key-rotation or connectivity problems."
    )]
    async fn test_connection(
        &self,
        Parameters(params): Parameters<TestConnectionParams>,
    ) -> CallToolResult {
        let alias = params.tribunal.unwrap_or_else(|| "trf1".to_owned());
        let Some(tribunal) = tribunals::find(&alias) else {
            return fail(&DatajudError::UnknownTribunal {
                alias: alias.clone(),
            });
        };
        return match self.client.ping(&tribunal.index_name()).await {
            Ok((response, elapsed)) => ok_json(&json!({
                "status": "ok",
                "tribunal": tribunal.alias,
                "index": tribunal.index_name(),
                "base_url": self.client.config().base_url,
                "latency_ms": u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
                "total_documents": response.get("hits").and_then(|node| node.get("total")),
                "shards": response.get("_shards"),
            })),
            Err(error) => fail(&error),
        };
    }
}
