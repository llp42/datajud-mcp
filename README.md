# datajud-mcp

MCP (Model Context Protocol) server for the **DATAJUD public API** (Conselho Nacional de Justiça — Brazil), written in **Rust**.

It exposes the complete public API surface — case metadata (*capas processuais*) and procedural movements (*movimentos*) from **all 91 tribunal indices** — as MCP tools, resources and prompts, over stdio.

- API documentation: <https://datajud-wiki.cnj.jus.br/api-publica/>
- Wire format: `POST https://api-publica.datajud.cnj.jus.br/api_publica_<alias>/_search` with search bodies in the Query DSL the CNJ documents as "padrão do Elasticsearch" (Elasticsearch-compatible **syntax** — no assumption is made about CNJ's infrastructure)

> **Naming convention:** all identifiers (tool names, parameters, output keys, code) are in **English**; the DATAJUD API request fields keep their official **PT-BR names** (`numeroProcesso`, `classe.codigo`, `orgaoJulgador.codigo`, `movimentos.dataHora`, `dataAjuizamento`, ...).

## Features — 100% of the documented API

| Capability | How |
|---|---|
| All 91 tribunal indices (superior, federal, state, labor, electoral, military courts) | `list_tribunals`, alias resolution on every tool |
| `POST _search` with the full documented query DSL | `search_dsl` (verbatim passthrough) + structured builders in `search_cases` / `get_case` / `aggregate_cases` |
| `POST _count` | `count_cases` |
| Aggregations (`terms`, `date_histogram`, ...) | `aggregate_cases`, or any `aggs` body via `search_dsl` |
| Pagination `size` (0–10,000) + `search_after` sorted by `@timestamp` | `search_cases` with `next_page` cursor in the response |
| `_source` field selection / movement trimming | `fields` + `include_movements` parameters |
| Public API key auth (`Authorization: APIKey ...`) | built in, overridable |
| Retries with backoff on 429/5xx/network errors | `DATAJUD_MAX_RETRIES` |
| Queryable field glossary | `list_fields`, `datajud://glossary` |
| Terms of use awareness | `datajud://terms-of-use` resource + server instructions |

## Requirements

- Rust 1.85+ (edition 2024) — only needed to build
- Outbound HTTPS access to `api-publica.datajud.cnj.jus.br`

## Build & install

```bash
cargo build --release
# binary at target/release/datajud-mcp

# or install into ~/.cargo/bin
cargo install --path .
```

## Configuration (environment variables)

| Variable | Default | Meaning |
|---|---|---|
| `DATAJUD_API_KEY` | public key from the wiki | credential sent as `Authorization: APIKey <key>` |
| `DATAJUD_BASE_URL` | `https://api-publica.datajud.cnj.jus.br` | API base URL |
| `DATAJUD_TIMEOUT_SECS` | `180` | per-request timeout (the API is slow — see below) |
| `DATAJUD_MAX_RETRIES` | `2` | retries on 429/5xx/network errors |
| `RUST_LOG` | `info` | logging (always to **stderr**; stdout is the MCP channel) |

The default key is the *Chave Pública* published at
<https://datajud-wiki.cnj.jus.br/api-publica/acesso>. **The CNJ may rotate it at any
time** — if tools return 401/403, fetch the current key from that page and set
`DATAJUD_API_KEY`.

## Use with MCP clients

### Claude Code

```bash
claude mcp add datajud -- /path/to/datajud-mcp
```

### Claude Desktop / Cursor / generic stdio client

```json
{
  "mcpServers": {
    "datajud": {
      "command": "/path/to/datajud-mcp",
      "env": {
        "DATAJUD_API_KEY": "cDZHYzlZa0JadVREZDJCendQbXY6SkJlTzNjLV9TRENyQk1RdnFKZGRQdw=="
      }
    }
  }
}
```

## Tools

### `list_tribunals`
Lists the 91 tribunal aliases (`tjsp`, `trf1`, `tre-sp`, `tst`, ...), each mapping to the index `api_publica_<alias>`.
- `segment` *(optional)*: `superior | federal | state | labor | electoral | military`
- `text` *(optional)*: free text over alias/name

### `list_fields`
Glossary of the queryable attributes: field path, field type (as listed in the official glossary), description — plus aggregatable fields, date formats and pagination notes. `text` filters by free text.

### `search_cases`
Structured search over one tribunal or a comma-separated list of up to 6 (politeness cap — the terms of use prohibit abusive consumption).

- `tribunal`: alias or `alias1,alias2,...`
- `filters`: all optional, AND-combined:
  `process_number`, `class_code`, `class_name`, `subject_code`, `subject_name`,
  `judging_body_code`, `judging_body_name`, `judging_body_municipality_ibge`,
  `instance`, `max_secrecy_level`, `system_code`, `system_name`,
  `format_code`, `format_name`, `filing_date_from`, `filing_date_until`,
  `updated_from`, `updated_until`, `movement_code`, `movement_name`,
  `movement_date_from`, `movement_date_until`, `free_text`
- `size` (default 10, max 10,000), `sort_field` (`timestamp` | `filing_date`), `sort_order` (`asc` | `desc`)
- `search_after`: pagination cursor = the `sort` array of the last hit of the previous page
- `include_movements` (default `false` — movement payloads are huge), `fields` (API `_source` allowlist)

Returns `cases` (id, score, sort, data), `total`, `took_ms`, `timed_out`, `shards`, and
`next_page` with a ready `search_after` cursor when more results exist.

### `get_case`
One full case by `process_number` (CNJ 20-digit number) or `id`, on a single `tribunal`.
`include_movements` defaults to `true` (full *movimentos* timeline).

### `count_cases`
`POST _count` with the same `filters` — sizes a population before searching. Supports up to 6 tribunals; returns `total` and `by_tribunal`.

### `aggregate_cases`
- `kind`: `terms` (top values of `field`) or `date_histogram` (over `date_field`)
- `field`: one of the aggregatable fields (`classe.codigo`, `orgaoJulgador.codigo`, `movimentos.codigo`, ...)
- `bucket_size`, `interval` (`day | week | month | quarter | year`), `date_field` (`dataAjuizamento` default | `dataHoraUltimaAtualizacao` | `@timestamp` | `movimentos.dataHora`)

### `search_dsl`
**Full escape hatch.** `tribunal` + `body` (any search body in the documented DSL) forwarded verbatim to `POST /api_publica_<alias>/_search`. Query, bool, range, nested-style movement filters, `aggs`, `highlight`, `sort`, `search_after`, `from`/`size`, `_source` — everything the API supports. Syntax reference (the format the CNJ docs point to): <https://www.elastic.co/guide/en/elasticsearch/reference/current/search-your-data.html>.

### `test_connection`
Liveness probe (size-0 `match_all` on one index, default `trf1`): status, latency, document count. Use it to diagnose key rotation or connectivity problems.

## Resources

| URI | Content |
|---|---|
| `datajud://tribunals` | full catalog of the 91 tribunals (markdown) |
| `datajud://tribunals/{alias}` | per-tribunal JSON details (resource template) |
| `datajud://glossary` | field glossary (markdown) |
| `datajud://examples` | official query examples + tool equivalents |
| `datajud://terms-of-use` | terms-of-use summary (acceptance is implied by API use) |

## Prompts

| Prompt | Purpose |
|---|---|
| `build_query` | design a step-by-step DATAJUD query plan from a research goal |
| `analyze_case` | fetch one case and interpret its metadata + movement timeline |
| `research_report` | produce a cited report (with the terms-of-use obligations for public reports) |

## Data model (API request fields, PT-BR)

Key queryable fields — full list via `list_fields` or `datajud://glossary`:

| Field | Type | Meaning |
|---|---|---|
| `numeroProcesso` | text/keyword | CNJ unified case number, unformatted (20 digits) |
| `dataAjuizamento` | datetime | filing date (stored as `AAAAMMDDHHMMSS`; tools accept `YYYY-MM-DD`) |
| `classe.codigo` / `classe.nome` | long / text | procedural class per TPU |
| `assuntos.codigo` / `assuntos.nome` | long / text | subject matter per TPU |
| `orgaoJulgador.codigo` / `.nome` / `.codigoMunicipioIBGE` | long / text / long | judging body (vara/serventia) + IBGE municipality |
| `movimentos.codigo` / `.nome` / `.dataHora` | long / text / datetime | procedural movements (TPU) + timestamp |
| `grau` | text/keyword | instance: `G1`, `G2`, `JE`, ... |
| `nivelSigilo` | long | secrecy level (0 = public) |
| `sistema.*`, `formato.*` | object | source system (e.g. `PJe`), physical/electronic format |
| `@timestamp` | datetime | index update timestamp — **use for `sort` + `search_after`** |

## Pagination

Per the official docs, prefer `search_after` over deep `from`:

```json
{ "size": 100,
  "query": { "bool": { "must": [ ... ] } },
  "sort": [ { "@timestamp": { "order": "asc" } } ],
  "search_after": [ 1681366085550 ] }
```

`search_cases` automates this: pass `search_after` from the previous response's
`next_page` and keep the same filters/sort. `size` > 10,000 is rejected by design.

## Performance & reliability notes

- Queries on large indices regularly take **20–40 seconds**; the default timeout is 180 s.
- The public API is intermittently overloaded (its responses report shard/thread-pool failures). Tool
  outputs include `timed_out` and `shards` so partial results are visible — **retry** when
  `found: false` / empty results look wrong.
- Multi-tribunal requests are capped at **6 aliases** per call.
- Keep `include_movements: false` unless you need the timeline — movement arrays dominate the payload.

## Terms of use

Consuming the API implies acceptance of the
[CNJ terms of use](https://formularios.cnj.jus.br/wp-content/uploads/2023/11/Termos-de-uso-api-publica-V1.2.pdf)
(`datajud://terms-of-use`):

- legal, **non-commercial** purposes only (3.3)
- no redistribution/commercial exploitation of the data or derived works (3.8)
- the CNJ does not guarantee accuracy or currency (3.6)
- **notify the CNJ** of any study/report/document published from the data (3.9)

Data comes from the Base Nacional de Dados do Poder Judiciário (Datajud), with classified
cases and party data protected (Portaria CNJ nº 160/2020).

## Development

```bash
cargo test                      # 18 unit tests (query DSL builders, catalog, glossary)
cargo clippy --all-targets      # lint (errors deny; see below)
cargo fmt --check               # formatting
python3 scripts/smoke.py        # offline MCP protocol smoke test (25 checks)
python3 scripts/smoke.py --live # + live calls against the public API (10 more checks)
```

### Lint policy — maximum strictness

The crate is built on **edition 2024** and linted with the strictest coherent
clippy configuration (see `[lints]` in `Cargo.toml`):

| Source | Level |
|---|---|
| rustc: `unsafe_code` | `forbid` |
| rustc: `missing_docs`, `missing_debug_implementations`, `unreachable_pub`, `unused`, `rust_2018_idioms`, `keyword_idents_2024`, `elided_lifetimes_in_paths`, `explicit_outlives_requirements` | `deny` |
| clippy groups: `pedantic`, `nursery`, `cargo` | `deny` |
| clippy::restriction | **all 135 lints enabled individually** minus 16 documented exclusions |

The exclusions are documented inline at the bottom of `[lints.clippy]` — every
one is either a **provable contradiction** inside the enabled set (e.g.
`implicit_return` vs `needless_return`; `unnecessary_rest_pattern` vs
`rest_pattern_accessible_field`; `min_ident_chars` vs `renamed_function_params`
over standard trait signatures) or a **domain requirement** (`non_ascii_literal`
— PT-BR tribunal names and API field names). The compile output was used as
evidence for each contradiction.

CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, the unit tests, the offline MCP
smoke test and a `cargo package` verification on every push and PR.

Project layout:

```
src/
├── main.rs            # stdio entry point (logs to stderr only)
├── lib.rs
├── config.rs          # env configuration
├── client.rs          # HTTP client (auth, timeout, retries)
├── query.rs           # English filters -> documented query DSL builders
├── tribunals.rs       # 91-tribunal catalog (src/tribunals_gen.rs is generated)
├── glossary.rs        # field glossary
├── error.rs           # errors with actionable hints
└── server/
    ├── mod.rs         # MCP wiring: info, resources, resource templates
    ├── tools.rs       # 8 tools
    └── prompts.rs     # 3 prompts
scripts/
├── smoke.py           # end-to-end MCP smoke test
└── gen_tribunals.py   # regenerates src/tribunals_gen.rs from the wiki catalog
```

## License

MIT
