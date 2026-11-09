//! Builders that translate friendly search parameters into the query DSL the
//! DATAJUD API accepts.
//!
//! The API accepts search bodies in the Query DSL documented by the CNJ
//! (Elasticsearch-compatible syntax: `query`, `size`, `sort`, `search_after`,
//! `aggs`, `_source`, ...). These helpers produce that body from structured
//! tool parameters, following the examples in
//! <https://datajud-wiki.cnj.jus.br/api-publica/exemplos/>.
//!
//! Tool parameters are named in English; the DATAJUD API request fields they
//! map to keep their official PT-BR names (`numeroProcesso`, `classe.codigo`, ...).

use serde::Serialize;
use serde_json::{Value, json};

use crate::error::DatajudError;
use crate::glossary::{AGGREGATABLE_FIELDS, DATE_FIELDS};

/// Maximum page size accepted by the API (documented limit).
pub const MAX_SIZE: u64 = 10_000;
/// Default page size (documented default).
pub const DEFAULT_SIZE: u64 = 10;

/// Structured filters shared by the search / count / aggregate tools.
///
/// All filters are optional and combined with AND semantics (`bool.must`).
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, schemars::JsonSchema)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
pub struct SearchFilters {
    /// CNJ unified case number (API field `numeroProcesso`), formatted or raw, e.g. `0003462-37.2022.4.05.8400` or `00034623720224058400`.
    pub process_number: Option<String>,
    /// Procedural class code per TPU (API field `classe.codigo`), e.g. 1116 for "Execução Fiscal".
    pub class_code: Option<i64>,
    /// Procedural class name (API field `classe.nome`) — phrase match.
    pub class_name: Option<String>,
    /// Subject-matter code per TPU (API field `assuntos.codigo`).
    pub subject_code: Option<i64>,
    /// Subject-matter name (API field `assuntos.nome`) — phrase match.
    pub subject_name: Option<String>,
    /// Judging body / court unit code (API field `orgaoJulgador.codigo`).
    pub judging_body_code: Option<i64>,
    /// Judging body / court unit name (API field `orgaoJulgador.nome`) — phrase match.
    pub judging_body_name: Option<String>,
    /// IBGE municipality code of the judging body (API field `orgaoJulgador.codigoMunicipioIBGE`).
    pub judging_body_municipality_ibge: Option<i64>,
    /// Instance/degree: "G1", "G2", "JE", etc. (API field `grau`).
    pub instance: Option<String>,
    /// Maximum secrecy level, inclusive (API field `nivelSigilo`); 0 = public.
    pub max_secrecy_level: Option<i64>,
    /// Procedural system code (API field `sistema.codigo`).
    pub system_code: Option<i64>,
    /// Procedural system name (API field `sistema.nome`), e.g. `PJe`.
    pub system_name: Option<String>,
    /// Case format code (API field `formato.codigo`).
    pub format_code: Option<i64>,
    /// Case format name (API field `formato.nome`): "Eletrônico" or "Físico".
    pub format_name: Option<String>,
    /// Filing date lower bound (API field `dataAjuizamento`), inclusive: "YYYY-MM-DD" or "AAAAMMDDHHMMSS".
    pub filing_date_from: Option<String>,
    /// Filing date upper bound (API field `dataAjuizamento`), inclusive: "YYYY-MM-DD" or "AAAAMMDDHHMMSS".
    pub filing_date_until: Option<String>,
    /// Last-update lower bound (API field `dataHoraUltimaAtualizacao`), ISO datetime passthrough.
    pub updated_from: Option<String>,
    /// Last-update upper bound (API field `dataHoraUltimaAtualizacao`), ISO datetime passthrough.
    pub updated_until: Option<String>,
    /// Procedural movement code per TPU (API field `movimentos.codigo`), e.g. 26 = "Distribuição".
    pub movement_code: Option<i64>,
    /// Procedural movement name (API field `movimentos.nome`) — phrase match.
    pub movement_name: Option<String>,
    /// Movement date lower bound (API field `movimentos.dataHora`).
    pub movement_date_from: Option<String>,
    /// Movement date upper bound (API field `movimentos.dataHora`).
    pub movement_date_until: Option<String>,
    /// Free text matched against `numeroProcesso`, `classe.nome`, `assuntos.nome`, `orgaoJulgador.nome`, `movimentos.nome`, `sistema.nome`, `formato.nome`.
    pub free_text: Option<String>,
}

/// Sortable fields for pagination.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    /// `@timestamp` — the field officially recommended for `search_after`.
    #[default]
    Timestamp,
    /// Case filing date (`dataAjuizamento`).
    FilingDate,
}

impl SortField {
    /// The API request field name this sort option maps to.
    #[must_use]
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Timestamp => "@timestamp",
            Self::FilingDate => "dataAjuizamento",
        }
    }
}

/// Sort direction.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    /// Ascending order.
    #[default]
    Asc,
    /// Descending order.
    Desc,
}

impl SortOrder {
    /// The API request value this sort order maps to.
    #[must_use]
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

/// Normalizes a human date into the compact `AAAAMMDDHHMMSS` form observed in the data.
///
/// * `"2024-01-31"` -> `"20240131000000"` when `end == false`.
/// * `"2024-01-31"` -> `"20240131235959"` when `end == true` (inclusive upper bound).
/// * `"20240131"`   -> same as above.
/// * Anything longer (full compact or ISO datetime) is passed through untouched.
///
/// # Errors
///
/// Returns [`DatajudError::InvalidInput`] when a digits-only value has neither
/// 8 nor 14 digits.
#[inline]
pub fn normalize_date(raw: &str, end: bool) -> Result<String, DatajudError> {
    let trimmed = raw.trim();
    let digits: String = trimmed.chars().filter(char::is_ascii_digit).collect();
    let only_digits = digits.len() == trimmed.chars().count();

    if only_digits {
        return match digits.len() {
            8 => Ok(format!("{digits}{}", if end { "235959" } else { "000000" })),
            14 => Ok(digits),
            count => Err(DatajudError::InvalidInput {
                message: format!(
                    "date '{raw}' has {count} digits; expected YYYY-MM-DD, YYYYMMDD or AAAAMMDDHHMMSS"
                ),
            }),
        };
    }

    // ISO-like "2024-01-31T12:00:00Z" — pass through unchanged.
    if trimmed
        .split_once('T')
        .is_some_and(|(_, rest)| !rest.is_empty())
    {
        return Ok(trimmed.to_owned());
    }
    let digit_count = trimmed.chars().filter(char::is_ascii_digit).count();
    match digit_count {
        8 => {
            let compact: String = trimmed.chars().filter(char::is_ascii_digit).collect();
            Ok(format!(
                "{compact}{}",
                if end { "235959" } else { "000000" }
            ))
        }
        // Anything else: pass through; the API accepts flexible date formats.
        _ => Ok(trimmed.to_owned()),
    }
}

/// Normalizes an optional date bound, preserving `None`.
///
/// # Errors
///
/// Propagates [`DatajudError::InvalidInput`] from [`normalize_date`].
fn optional_date(raw: Option<&str>, end: bool) -> Result<Option<String>, DatajudError> {
    raw.map_or(Ok(None), |text| normalize_date(text, end).map(Some))
}

/// Builds a `match` clause for analyzed text fields.
fn match_clause(field: &str, value: &str) -> Value {
    json!({ "match": { field: value } })
}

/// Builds a `match_phrase` clause for exact-phrase name fields.
fn phrase_clause(field: &str, value: &str) -> Value {
    json!({ "match_phrase": { field: value } })
}

/// Builds a `term` clause for exact numeric/keyword fields.
fn term_clause(field: &str, value: &Value) -> Value {
    json!({ "term": { field: value } })
}

/// Builds a `range` clause with optional inclusive bounds.
fn range_clause(field: &str, gte: Option<String>, lte: Option<String>) -> Value {
    let mut range = serde_json::Map::new();
    if let Some(lower) = gte {
        range.insert("gte".to_owned(), Value::String(lower));
    }
    if let Some(upper) = lte {
        range.insert("lte".to_owned(), Value::String(upper));
    }
    json!({ "range": { field: range } })
}

/// Builds the `bool` query for the given filters (`match_all` when empty).
///
/// # Errors
///
/// Returns [`DatajudError::InvalidInput`] when a date bound cannot be normalized.
#[inline]
pub fn build_query(filters: &SearchFilters) -> Result<Value, DatajudError> {
    let mut must: Vec<Value> = Vec::new();

    if let Some(value) = &filters.process_number {
        must.push(match_clause("numeroProcesso", value));
    }
    if let Some(value) = filters.class_code {
        must.push(term_clause("classe.codigo", &json!(value)));
    }
    if let Some(value) = &filters.class_name {
        must.push(phrase_clause("classe.nome", value));
    }
    if let Some(value) = filters.subject_code {
        must.push(term_clause("assuntos.codigo", &json!(value)));
    }
    if let Some(value) = &filters.subject_name {
        must.push(phrase_clause("assuntos.nome", value));
    }
    if let Some(value) = filters.judging_body_code {
        must.push(term_clause("orgaoJulgador.codigo", &json!(value)));
    }
    if let Some(value) = &filters.judging_body_name {
        must.push(phrase_clause("orgaoJulgador.nome", value));
    }
    if let Some(value) = filters.judging_body_municipality_ibge {
        must.push(term_clause(
            "orgaoJulgador.codigoMunicipioIBGE",
            &json!(value),
        ));
    }
    if let Some(value) = &filters.instance {
        must.push(match_clause("grau", value));
    }
    if let Some(value) = filters.max_secrecy_level {
        must.push(range_clause("nivelSigilo", None, Some(value.to_string())));
    }
    if let Some(value) = filters.system_code {
        must.push(term_clause("sistema.codigo", &json!(value)));
    }
    if let Some(value) = &filters.system_name {
        must.push(phrase_clause("sistema.nome", value));
    }
    if let Some(value) = filters.format_code {
        must.push(term_clause("formato.codigo", &json!(value)));
    }
    if let Some(value) = &filters.format_name {
        must.push(phrase_clause("formato.nome", value));
    }

    if filters.filing_date_from.is_some() || filters.filing_date_until.is_some() {
        let gte = optional_date(filters.filing_date_from.as_deref(), false)?;
        let lte = optional_date(filters.filing_date_until.as_deref(), true)?;
        must.push(range_clause("dataAjuizamento", gte, lte));
    }
    if filters.updated_from.is_some() || filters.updated_until.is_some() {
        must.push(range_clause(
            "dataHoraUltimaAtualizacao",
            filters.updated_from.clone(),
            filters.updated_until.clone(),
        ));
    }
    if filters.movement_date_from.is_some() || filters.movement_date_until.is_some() {
        must.push(range_clause(
            "movimentos.dataHora",
            filters.movement_date_from.clone(),
            filters.movement_date_until.clone(),
        ));
    }
    if let Some(value) = filters.movement_code {
        must.push(term_clause("movimentos.codigo", &json!(value)));
    }
    if let Some(value) = &filters.movement_name {
        must.push(phrase_clause("movimentos.nome", value));
    }

    if let Some(value) = &filters.free_text {
        must.push(json!({
            "multi_match": {
                "query": value,
                "type": "best_fields",
                "fields": [
                    "numeroProcesso",
                    "classe.nome",
                    "assuntos.nome",
                    "orgaoJulgador.nome",
                    "movimentos.nome",
                    "sistema.nome",
                    "formato.nome"
                ]
            }
        }));
    }

    if must.is_empty() {
        return Ok(json!({ "match_all": {} }));
    }
    Ok(json!({ "bool": { "must": must } }))
}

/// Builds the `_source` clause: either an include list, an exclusion of heavy
/// movement payloads, or nothing at all (full document).
#[must_use]
#[inline]
pub fn build_source(fields: Option<&[String]>, include_movements: bool) -> Option<Value> {
    if let Some(include) = fields {
        if !include.is_empty() {
            return Some(Value::Array(
                include
                    .iter()
                    .map(|field| Value::String(field.clone()))
                    .collect(),
            ));
        }
    }
    if !include_movements {
        return Some(json!({ "excludes": ["movimentos"] }));
    }
    None
}

/// Validates page size against the documented limits (`0..=10000`).
///
/// # Errors
///
/// Returns [`DatajudError::InvalidInput`] when `size` exceeds [`MAX_SIZE`].
#[inline]
pub fn validate_size(size: u64) -> Result<u64, DatajudError> {
    if size > MAX_SIZE {
        return Err(DatajudError::InvalidInput {
            message: format!(
                "size {size} exceeds the API limit of {MAX_SIZE} per page; use search_after for deep pagination"
            ),
        });
    }
    Ok(size)
}

/// Full `_search` body from structured parameters.
///
/// # Errors
///
/// Returns [`DatajudError::InvalidInput`] when `size` is out of range or
/// `search_after` is not a JSON array.
#[inline]
pub fn build_search_body(
    filters: &SearchFilters,
    size: u64,
    sort_field: SortField,
    sort_order: SortOrder,
    search_after: Option<&Value>,
    fields: Option<&[String]>,
    include_movements: bool,
) -> Result<Value, DatajudError> {
    let checked_size = validate_size(size)?;
    let query = build_query(filters)?;
    let mut body = serde_json::Map::new();
    body.insert("size".to_owned(), json!(checked_size));
    body.insert("query".to_owned(), query);
    body.insert(
        "sort".to_owned(),
        json!([ { sort_field.as_str(): { "order": sort_order.as_str() } } ]),
    );
    if let Some(source) = build_source(fields, include_movements) {
        body.insert("_source".to_owned(), source);
    }
    if let Some(cursor) = search_after {
        if !cursor.is_array() {
            return Err(DatajudError::InvalidInput {
                message: "search_after must be a JSON array (the 'sort' values of the last hit of the previous page)".to_owned(),
            });
        }
        body.insert("search_after".to_owned(), cursor.clone());
    }
    Ok(Value::Object(body))
}

/// `_count` body (query only).
///
/// # Errors
///
/// Returns [`DatajudError::InvalidInput`] when a date bound cannot be normalized.
#[inline]
pub fn build_count_body(filters: &SearchFilters) -> Result<Value, DatajudError> {
    let query = build_query(filters)?;
    Ok(json!({ "query": query }))
}

/// Which aggregation the `aggregate_cases` tool should run.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum AggKind {
    /// Terms aggregation: most frequent values of a field.
    Terms,
    /// Date histogram over `dataAjuizamento` / `dataHoraUltimaAtualizacao` / `@timestamp` / `movimentos.dataHora`.
    DateHistogram,
}

/// Builds a `_search` body with a single aggregation and `size: 0`.
///
/// # Errors
///
/// Returns [`DatajudError::InvalidInput`] when the aggregation field is not in
/// the aggregable/date field allow-lists, or when a date bound is invalid.
#[inline]
pub fn build_agg_body(
    filters: &SearchFilters,
    field: &str,
    kind: AggKind,
    bucket_size: u64,
    interval: Option<&str>,
    date_field: Option<&str>,
) -> Result<Value, DatajudError> {
    let query = build_query(filters)?;
    match kind {
        AggKind::Terms => {
            if !AGGREGATABLE_FIELDS.contains(&field) {
                return Err(DatajudError::InvalidInput {
                    message: format!(
                        "field '{field}' is not aggregable; valid fields: {}",
                        AGGREGATABLE_FIELDS.join(", ")
                    ),
                });
            }
            Ok(json!({
                "size": 0,
                "query": query,
                "aggs": {
                    "aggregation": {
                        "terms": { "field": field, "size": bucket_size }
                    }
                }
            }))
        }
        AggKind::DateHistogram => {
            let chosen = date_field.unwrap_or("dataAjuizamento");
            if !DATE_FIELDS.contains(&chosen) && chosen != "movimentos.dataHora" {
                return Err(DatajudError::InvalidInput {
                    message: format!(
                        "date field '{chosen}' is invalid; valid fields: {}, movimentos.dataHora",
                        DATE_FIELDS.join(", ")
                    ),
                });
            }
            let chosen_interval = interval.unwrap_or("year");
            Ok(json!({
                "size": 0,
                "query": query,
                "aggs": {
                    "aggregation": {
                        "date_histogram": {
                            "field": chosen,
                            "calendar_interval": chosen_interval,
                            "format": "yyyy-MM-dd"
                        }
                    }
                }
            }))
        }
    }
}

/// Renders a short human note explaining how to fetch the next page, if any.
#[must_use]
#[inline]
pub fn next_page_hint(sort_values: Option<&Value>, size: u64, returned: usize) -> Option<Value> {
    if u64::try_from(returned).unwrap_or(u64::MAX) < size {
        return None;
    }
    sort_values.map(|cursor| {
        json!({
            "search_after": cursor,
            "instruction": "Pass this 'search_after' array (plus the same sort) to the next call to fetch the following page."
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_dates() {
        assert_eq!(
            normalize_date("2024-01-31", false).unwrap(),
            "20240131000000",
            "lower bound"
        );
        assert_eq!(
            normalize_date("2024-01-31", true).unwrap(),
            "20240131235959",
            "upper bound"
        );
        assert_eq!(
            normalize_date("20240131", true).unwrap(),
            "20240131235959",
            "compact date"
        );
        assert_eq!(
            normalize_date("20240131000000", false).unwrap(),
            "20240131000000",
            "passthrough"
        );
        assert_eq!(
            normalize_date("2024-01-31T12:00:00Z", false).unwrap(),
            "2024-01-31T12:00:00Z",
            "ISO datetime passthrough"
        );
        assert!(
            normalize_date("2024", false).is_err(),
            "bad length must be rejected"
        );
    }

    #[test]
    fn empty_filters_produce_match_all() {
        let query = build_query(&SearchFilters::default()).unwrap();
        assert_eq!(
            query,
            json!({ "match_all": {} }),
            "no filters means match_all"
        );
    }

    #[test]
    fn document_example_1_by_process_number() {
        // The official wiki example 1 (match numeroProcesso).
        let filters = SearchFilters {
            process_number: Some("00008323520184013202".to_owned()),
            ..Default::default()
        };
        let query = build_query(&filters).unwrap();
        assert_eq!(
            query,
            json!({ "bool": { "must": [ { "match": { "numeroProcesso": "00008323520184013202" } } ] } }),
            "wiki example 1"
        );
    }

    #[test]
    fn document_example_2_by_class_and_court() {
        // The official wiki example 2 (classe.codigo + orgaoJulgador.codigo).
        let filters = SearchFilters {
            class_code: Some(1116),
            judging_body_code: Some(13597),
            ..Default::default()
        };
        let query = build_query(&filters).unwrap();
        let must = query["bool"]["must"].as_array().unwrap();
        assert_eq!(must.len(), 2, "two clauses");
        assert!(
            must.contains(&json!({ "term": { "classe.codigo": 1116 } })),
            "class clause"
        );
        assert!(
            must.contains(&json!({ "term": { "orgaoJulgador.codigo": 13597 } })),
            "court clause"
        );
    }

    #[test]
    fn date_range_uses_compact_format() {
        let filters = SearchFilters {
            filing_date_from: Some("2024-01-01".to_owned()),
            filing_date_until: Some("2024-01-02".to_owned()),
            ..Default::default()
        };
        let query = build_query(&filters).unwrap();
        assert_eq!(
            query["bool"]["must"][0]["range"]["dataAjuizamento"],
            json!({ "gte": "20240101000000", "lte": "20240102235959" }),
            "dates are compacted inclusively"
        );
    }

    #[test]
    fn secrecy_level_is_an_upper_bound() {
        let filters = SearchFilters {
            max_secrecy_level: Some(0),
            ..Default::default()
        };
        let query = build_query(&filters).unwrap();
        assert_eq!(
            query["bool"]["must"][0]["range"]["nivelSigilo"],
            json!({ "lte": "0" }),
            "secrecy is bounded above"
        );
    }

    #[test]
    fn search_body_sets_sort_and_source() {
        let body = build_search_body(
            &SearchFilters::default(),
            100,
            SortField::Timestamp,
            SortOrder::Asc,
            Some(&json!([1_681_366_085_550_i64])),
            None,
            false,
        )
        .unwrap();
        assert_eq!(body["size"], 100, "size passthrough");
        assert_eq!(
            body["sort"],
            json!([{ "@timestamp": { "order": "asc" } }]),
            "sort"
        );
        assert_eq!(
            body["search_after"],
            json!([1_681_366_085_550_i64]),
            "cursor"
        );
        assert_eq!(
            body["_source"],
            json!({ "excludes": ["movimentos"] }),
            "trimmed source"
        );
    }

    #[test]
    fn size_validation() {
        assert!(validate_size(10_000).is_ok(), "boundary is allowed");
        assert!(validate_size(10_001).is_err(), "over the limit is rejected");
    }

    #[test]
    fn search_after_must_be_array() {
        let result = build_search_body(
            &SearchFilters::default(),
            10,
            SortField::Timestamp,
            SortOrder::Asc,
            Some(&json!("oops")),
            None,
            true,
        );
        assert!(result.is_err(), "non-array cursor must be rejected");
    }

    #[test]
    fn agg_body_terms_and_histogram() {
        let terms = build_agg_body(
            &SearchFilters::default(),
            "classe.codigo",
            AggKind::Terms,
            10,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            terms["aggs"]["aggregation"]["terms"]["field"], "classe.codigo",
            "terms field"
        );
        assert_eq!(terms["size"], 0, "aggregations do not fetch hits");

        let histogram = build_agg_body(
            &SearchFilters::default(),
            "dataAjuizamento",
            AggKind::DateHistogram,
            10,
            Some("year"),
            Some("dataAjuizamento"),
        )
        .unwrap();
        assert_eq!(
            histogram["aggs"]["aggregation"]["date_histogram"]["calendar_interval"], "year",
            "histogram interval"
        );

        assert!(
            build_agg_body(
                &SearchFilters::default(),
                "hacker.field",
                AggKind::Terms,
                10,
                None,
                None
            )
            .is_err(),
            "unknown aggregation fields must be rejected"
        );
    }

    #[test]
    fn free_text_uses_multi_match() {
        let filters = SearchFilters {
            free_text: Some("dano moral".to_owned()),
            ..Default::default()
        };
        let query = build_query(&filters).unwrap();
        assert_eq!(
            query["bool"]["must"][0]["multi_match"]["query"], "dano moral",
            "free text clause"
        );
    }
}
