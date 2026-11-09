//! Static catalog of the 91 tribunal indices served by the DATAJUD public API.
//!
//! Every tribunal exposes an index named `api_publica_<alias>` under the API
//! base URL. The catalog mirrors the official table at
//! <https://datajud-wiki.cnj.jus.br/api-publica/endpoints>.

include!("tribunals_gen.rs");

/// Justice segment a tribunal belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Segment {
    /// Superior courts (`tst`, `tse`, `stj`, `stm`).
    Superior,
    /// Federal justice (`trf1`..`trf6`).
    Federal,
    /// State justice (`tjac`..`tjto`).
    State,
    /// Labor justice (`trt1`..`trt24`).
    Labor,
    /// Electoral justice (`tre-ac`..`tre-to`).
    Electoral,
    /// Military justice (`tjmmg`, `tjmrs`, `tjmsp`).
    Military,
}

impl Segment {
    /// Stable lowercase identifier of the segment, as used by tool parameters.
    #[must_use]
    #[inline]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Superior => "superior",
            Self::Federal => "federal",
            Self::State => "state",
            Self::Labor => "labor",
            Self::Electoral => "electoral",
            Self::Military => "military",
        }
    }

    /// Human-readable segment label in English.
    #[must_use]
    #[inline]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Superior => "Superior Courts",
            Self::Federal => "Federal Justice",
            Self::State => "State Justice",
            Self::Labor => "Labor Justice",
            Self::Electoral => "Electoral Justice",
            Self::Military => "Military Justice",
        }
    }

    /// Parses a segment filter value.
    ///
    /// Accepts the English names plus the Portuguese equivalents from the
    /// official documentation. Returns `None` for unknown values.
    #[must_use]
    #[inline]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "superior" | "superiors" | "tribunais_superiores" => Some(Self::Superior),
            "federal" | "justica_federal" => Some(Self::Federal),
            "state" | "estadual" | "justica_estadual" => Some(Self::State),
            "labor" | "trabalho" | "justica_trabalho" | "labour" => Some(Self::Labor),
            "electoral" | "eleitoral" | "justica_eleitoral" => Some(Self::Electoral),
            "military" | "militar" | "justica_militar" => Some(Self::Military),
            _ => None,
        }
    }
}

/// One tribunal index in the public API.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Tribunal {
    /// Index alias, e.g. `tjsp` (the index is `api_publica_tjsp`).
    pub alias: &'static str,
    /// Official tribunal name (proper noun, kept as published by the CNJ).
    pub name: &'static str,
    /// Justice segment.
    pub segment: Segment,
}

impl Tribunal {
    /// Index name, e.g. `api_publica_tjsp`.
    #[must_use]
    #[inline]
    pub fn index_name(&self) -> String {
        format!("api_publica_{}", self.alias)
    }

    /// Full `_search` URL for this tribunal under the given base URL.
    #[must_use]
    #[inline]
    pub fn search_url(&self, base_url: &str) -> String {
        format!(
            "{}/{}/_search",
            base_url.trim_end_matches('/'),
            self.index_name()
        )
    }

    /// Full `_count` URL for this tribunal under the given base URL.
    #[must_use]
    #[inline]
    pub fn count_url(&self, base_url: &str) -> String {
        format!(
            "{}/{}/_count",
            base_url.trim_end_matches('/'),
            self.index_name()
        )
    }
}

/// Resolves an alias (case-insensitive) to a tribunal.
#[must_use]
#[inline]
pub fn find(alias: &str) -> Option<&'static Tribunal> {
    let wanted = alias.trim().to_ascii_lowercase();
    TRIBUNALS.iter().find(|tribunal| tribunal.alias == wanted)
}

/// Filters the catalog by optional segment and free-text query (matches alias or name).
#[must_use]
#[inline]
pub fn search(segment: Option<Segment>, text: Option<&str>) -> Vec<&'static Tribunal> {
    let needle = text
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    TRIBUNALS
        .iter()
        .filter(|tribunal| segment.is_none_or(|seg| tribunal.segment == seg))
        .filter(|tribunal| {
            needle.is_empty()
                || tribunal.alias.contains(&needle)
                || tribunal.name.to_ascii_lowercase().contains(&needle)
        })
        .collect()
}

/// Suggests the closest known alias for a mistyped tribunal name
/// (Levenshtein distance of at most 2).
#[must_use]
#[inline]
pub fn suggest(alias: &str) -> Option<&'static Tribunal> {
    let wanted = alias.trim().to_ascii_lowercase();
    let mut best: Option<(usize, &'static Tribunal)> = None;
    for tribunal in TRIBUNALS {
        let distance = levenshtein(&wanted, tribunal.alias);
        if distance <= 2 && best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, tribunal));
        }
    }
    best.map(|(_, tribunal)| tribunal)
}

/// Levenshtein edit distance between two strings.
fn levenshtein(left: &str, right: &str) -> usize {
    let left_chars: Vec<char> = left.chars().collect();
    let right_chars: Vec<char> = right.chars().collect();
    let right_len = right_chars.len();
    let mut previous: Vec<usize> = (0..=right_len).collect();
    for (row, left_char) in left_chars.iter().enumerate() {
        let mut current: Vec<usize> = Vec::with_capacity(right_len.saturating_add(1));
        current.push(row.saturating_add(1));
        let mut left_value = row.saturating_add(1);
        let mut diagonal = row;
        for (column, right_char) in right_chars.iter().enumerate() {
            let above = previous
                .get(column.saturating_add(1))
                .copied()
                .unwrap_or_default();
            let cost = usize::from(left_char != right_char);
            let insert = left_value.saturating_add(1);
            let delete = above.saturating_add(1);
            let substitute = diagonal.saturating_add(cost);
            let value = insert.min(delete).min(substitute);
            current.push(value);
            diagonal = above;
            left_value = value;
        }
        previous = current;
    }
    previous.last().copied().unwrap_or(left_chars.len())
}

/// Number of registered tribunals.
#[must_use]
#[inline]
pub const fn total() -> usize {
    TRIBUNALS.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_complete() {
        // 4 superiores + 6 TRFs + 27 TJs + 24 TRTs + 27 TREs + 3 TJMs.
        assert_eq!(total(), 91, "the catalog must mirror the 91 wiki endpoints");
    }

    #[test]
    fn lookup_is_case_insensitive() {
        assert_eq!(
            find("TJSP").unwrap().alias,
            "tjsp",
            "alias lookup must fold case"
        );
        assert_eq!(
            find(" tre-sp ").unwrap().alias,
            "tre-sp",
            "alias lookup must trim"
        );
        assert!(
            find("nao-existe").is_none(),
            "unknown aliases must be rejected"
        );
    }

    #[test]
    fn urls_follow_documented_pattern() {
        let tribunal = find("trf1").unwrap();
        assert_eq!(
            tribunal.index_name(),
            "api_publica_trf1",
            "index naming pattern"
        );
        assert_eq!(
            tribunal.search_url("https://api-publica.datajud.cnj.jus.br/"),
            "https://api-publica.datajud.cnj.jus.br/api_publica_trf1/_search",
            "search URL pattern from the wiki"
        );
    }

    #[test]
    fn suggests_close_aliases() {
        assert_eq!(
            suggest("trff5").map(|tribunal| tribunal.alias),
            Some("trf5"),
            "typo suggestion"
        );
        assert_eq!(
            suggest("TJSP ").map(|tribunal| tribunal.alias),
            Some("tjsp"),
            "case/space"
        );
        assert_eq!(
            suggest("tjspp").map(|tribunal| tribunal.alias),
            Some("tjsp"),
            "typo"
        );
        assert!(suggest("zzzzzzzz").is_none(), "nothing close enough");
    }

    #[test]
    fn segment_filter_works() {
        assert_eq!(
            search(Some(Segment::Military), None).len(),
            3,
            "military courts"
        );
        assert_eq!(
            search(Some(Segment::Superior), None).len(),
            4,
            "superior courts"
        );
        assert_eq!(
            search(None, Some("são paulo")).len(),
            3,
            "TJSP + TRE-SP + TJMSP"
        );
    }
}
