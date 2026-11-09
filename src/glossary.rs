//! Data glossary of the DATAJUD public API documents.
//!
//! Mirrors <https://datajud-wiki.cnj.jus.br/api-publica/glossario>. Each case
//! ("capa processual") is a JSON document with these attributes.
//! Field names are the PT-BR names used by the API requests — do not translate
//! them when building queries.

use std::fmt::Write as _;

/// One field of the case metadata schema.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct SchemaField {
    /// Field path as used in API queries (PT-BR, verbatim API name).
    pub name: &'static str,
    /// Type as listed in the official glossary (`text/keyword`, `long`, `datetime`).
    pub field_type: &'static str,
    /// Human description.
    pub description: &'static str,
}

/// All fields of the case metadata schema, in glossary order.
pub static FIELDS: &[SchemaField] = &[
    SchemaField {
        name: "id",
        field_type: "text/keyword",
        description: "Case identifier in Datajud — key Tribunal_Classe_Grau_OrgaoJulgador_NumeroProcesso",
    },
    SchemaField {
        name: "tribunal",
        field_type: "text/keyword",
        description: "Tribunal acronym (sigla)",
    },
    SchemaField {
        name: "numeroProcesso",
        field_type: "text/keyword",
        description: "CNJ unified case number, unformatted (20 digits)",
    },
    SchemaField {
        name: "dataAjuizamento",
        field_type: "datetime",
        description: "Case filing date (observed format: AAAAMMDDHHMMSS, e.g. 20181029000000)",
    },
    SchemaField {
        name: "grau",
        field_type: "text/keyword",
        description: "Instance/degree (G1, G2, JE, etc.)",
    },
    SchemaField {
        name: "nivelSigilo",
        field_type: "long",
        description: "Secrecy level (0 = public)",
    },
    SchemaField {
        name: "formato.codigo",
        field_type: "long",
        description: "Case format code",
    },
    SchemaField {
        name: "formato.nome",
        field_type: "text/keyword",
        description: "Case format name: 'Eletrônico' (electronic) or 'Físico' (physical)",
    },
    SchemaField {
        name: "sistema.codigo",
        field_type: "long",
        description: "Procedural system code",
    },
    SchemaField {
        name: "sistema.nome",
        field_type: "text/keyword",
        description: "Procedural system name (e.g. 'PJe')",
    },
    SchemaField {
        name: "classe.codigo",
        field_type: "long",
        description: "Procedural class code per TPU",
    },
    SchemaField {
        name: "classe.nome",
        field_type: "text/keyword",
        description: "Procedural class name",
    },
    SchemaField {
        name: "assuntos.codigo",
        field_type: "long",
        description: "Subject-matter code per TPU",
    },
    SchemaField {
        name: "assuntos.nome",
        field_type: "text/keyword",
        description: "Subject-matter name",
    },
    SchemaField {
        name: "orgaoJulgador.codigo",
        field_type: "long",
        description: "Code of the current judging body (court unit/vara)",
    },
    SchemaField {
        name: "orgaoJulgador.nome",
        field_type: "text/keyword",
        description: "Name of the judging body (court unit/vara)",
    },
    SchemaField {
        name: "orgaoJulgador.codigoMunicipioIBGE",
        field_type: "long",
        description: "Municipality of the judging body, IBGE code",
    },
    SchemaField {
        name: "movimentos.codigo",
        field_type: "long",
        description: "Procedural movement code per TPU",
    },
    SchemaField {
        name: "movimentos.nome",
        field_type: "text/keyword",
        description: "Procedural movement name",
    },
    SchemaField {
        name: "movimentos.dataHora",
        field_type: "datetime",
        description: "Date and time of the movement",
    },
    SchemaField {
        name: "movimentos.complementosTabelados.codigo",
        field_type: "long",
        description: "Code of the tabulated movement variable",
    },
    SchemaField {
        name: "movimentos.complementosTabelados.descricao",
        field_type: "text/keyword",
        description: "Description of the tabulated movement variable",
    },
    SchemaField {
        name: "movimentos.complementosTabelados.valor",
        field_type: "long",
        description: "Code of the tabulated complement",
    },
    SchemaField {
        name: "movimentos.complementosTabelados.nome",
        field_type: "text/keyword",
        description: "Description of the tabulated complement",
    },
    SchemaField {
        name: "movimentos.orgaoJulgador.codigoOrgao",
        field_type: "long",
        description: "Code of the court unit of the movement",
    },
    SchemaField {
        name: "movimentos.orgaoJulgador.nomeOrgao",
        field_type: "text/keyword",
        description: "Name of the court unit of the movement",
    },
    SchemaField {
        name: "dataHoraUltimaAtualizacao",
        field_type: "datetime",
        description: "Source insertion millis (internal control field)",
    },
    SchemaField {
        name: "@timestamp",
        field_type: "datetime",
        description: "Document update timestamp in the index (internal control; use for sort/search_after pagination)",
    },
];

/// Fields usable in date-range filters of the friendly search tools.
pub static DATE_FIELDS: &[&str] = &["dataAjuizamento", "dataHoraUltimaAtualizacao", "@timestamp"];

/// Fields accepted by the `aggregate_cases` tool for terms/date-histogram aggregations.
pub static AGGREGATABLE_FIELDS: &[&str] = &[
    "classe.codigo",
    "classe.nome",
    "assuntos.codigo",
    "assuntos.nome",
    "orgaoJulgador.codigo",
    "orgaoJulgador.nome",
    "orgaoJulgador.codigoMunicipioIBGE",
    "movimentos.codigo",
    "movimentos.nome",
    "sistema.codigo",
    "sistema.nome",
    "formato.codigo",
    "formato.nome",
    "grau",
    "nivelSigilo",
    "tribunal",
];

/// Filters the glossary by free-text (matches name or description).
#[must_use]
#[inline]
pub fn search(text: Option<&str>) -> Vec<&'static SchemaField> {
    let needle = text
        .map(|value| return value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    return FIELDS
        .iter()
        .filter(|field| {
            return needle.is_empty()
                || field.name.to_ascii_lowercase().contains(&needle)
                || field.description.to_ascii_lowercase().contains(&needle);
        })
        .collect();
}

/// Renders the full glossary as Markdown (used by the `datajud://glossary` resource).
#[must_use]
#[inline]
pub fn render_markdown() -> String {
    let mut out = String::from(
        "# Data Glossary — DATAJUD Public API\n\n\
         Source: https://datajud-wiki.cnj.jus.br/api-publica/glossario\n\n\
         Each case is a JSON document with the attributes below.\n\
         Field names are the PT-BR API names — use them verbatim in queries.\n\n\
         | Field | Type | Description |\n|---|---|---|\n",
    );
    for field in FIELDS {
        writeln!(
            out,
            "| `{}` | {} | {} |",
            field.name, field.field_type, field.description
        )
        .unwrap_or(());
    }
    out.push_str(
        "\n## Query notes\n\n\
         - `dataAjuizamento` appears in the data as `AAAAMMDDHHMMSS` (e.g. `20181029000000`); the tools accept plain `YYYY-MM-DD` dates and convert them automatically.\n\
         - For deep pagination use `sort` on `@timestamp` + `search_after` (recommended by the official docs).\n\
         - `size` per page: 0 to 10,000 (default 10).\n\
         - Aggregations (`aggs`) use the query syntax documented by the CNJ (Elasticsearch-compatible); reference guide: https://www.elastic.co/guide/en/elasticsearch/reference/current/search-your-data.html\n",
    );
    return out;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glossary_matches_documented_fields() {
        assert!(FIELDS.len() >= 28, "the glossary mirrors the wiki table");
        assert!(
            FIELDS.iter().any(|field| field.name == "numeroProcesso"),
            "numeroProcesso must be documented"
        );
        assert!(
            FIELDS.iter().any(|field| field.name == "@timestamp"),
            "@timestamp must be documented"
        );
    }

    #[test]
    fn markdown_renders() {
        let rendered = render_markdown();
        assert!(
            rendered.contains("| `numeroProcesso` |"),
            "table rows must render"
        );
        assert!(
            rendered.contains("search_after"),
            "pagination notes must render"
        );
    }
}
