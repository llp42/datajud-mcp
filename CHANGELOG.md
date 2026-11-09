# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-11-09

### Added

- CNJ process number handling: `get_case` and the `process_number` filter
  accept formatted numbers (`0003462-37.2022.4.05.8400`) or raw 20 digits.
- Tribunal inference: when `tribunal` is omitted, `get_case` derives it from
  the number's J/TR fields (federal, labor, electoral and state courts);
  when both are given they must agree, and a mismatch produces a clear error.
- Check-digit validation (Resolução CNJ 65/2008): mismatches are surfaced as
  `number_warning` with the expected digits.
- "Did you mean" suggestions for mistyped tribunal aliases.

### Changed

- **Clearer errors for models:** unknown tool parameters are now rejected
  (`deny_unknown_fields`) instead of being silently ignored, and error
  messages state what was received, what is expected and how to retry.

## [0.2.0] - 2026-11-09

### Changed

- **Breaking:** the API key is no longer embedded in the binary. Set the
  `DATAJUD_API_KEY` environment variable (required) — in your MCP client's
  `env` block, not as a tool parameter. Get the public key at
  <https://datajud-wiki.cnj.jus.br/api-publica/acesso>. This keeps secrets out
  of model context/transcripts and lets key rotation happen without a
  republish.

## [0.1.0] - 2026-11-09

### Added

- MCP server (stdio) for the DATAJUD public API (CNJ) with 8 tools:
  `list_tribunals`, `list_fields`, `search_cases`, `get_case`, `count_cases`,
  `aggregate_cases`, `search_dsl` (raw query DSL passthrough) and
  `test_connection`.
- Catalog of all 91 tribunal indices, generated from the official wiki.
- Resources: `datajud://tribunals`, `datajud://glossary`, `datajud://examples`,
  `datajud://terms-of-use`, plus the `datajud://tribunals/{alias}` template.
- Prompts: `build_query`, `analyze_case`, `research_report`.
- Library API (`datajud_mcp`) for direct queries against the API.
- Maximum-strict lint configuration (clippy pedantic + nursery + cargo +
  individual restriction lints) and a CI workflow.
