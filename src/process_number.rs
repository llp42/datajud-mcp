//! CNJ unified process number (`numeroProcesso`) utilities.
//!
//! The CNJ numbering format is `NNNNNNN-DD.AAAA.J.TR.OOOO` (20 digits):
//! sequential number, check digits, filing year, judiciary segment, tribunal
//! and origin court unit. This module accepts formatted or raw numbers,
//! validates the check digits and infers the tribunal alias from the number
//! itself — so a model or user can pass `"0003462-37.2022.4.05.8400"` without
//! manually stripping punctuation or guessing the tribunal.

use crate::error::DatajudError;
use crate::tribunals;

/// Number of digits in a CNJ unified process number.
pub const DIGITS: usize = 20;

/// A validated CNJ unified process number, stored without punctuation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProcessNumber {
    /// The 20 digits without punctuation, exactly as the API stores them.
    pub digits: String,
}

impl ProcessNumber {
    /// Parses a formatted or raw process number.
    ///
    /// All non-digit characters are ignored (dots, dashes, spaces), so both
    /// `"0003462-37.2022.4.05.8400"` and `"00034623720224058400"` are accepted.
    ///
    /// # Errors
    ///
    /// Returns [`DatajudError::InvalidInput`] when the input does not contain
    /// exactly 20 digits, with a message that shows what was found.
    #[inline]
    pub fn parse(input: &str) -> Result<Self, DatajudError> {
        let digits: String = input.chars().filter(char::is_ascii_digit).collect();
        if digits.len() != DIGITS {
            return Err(DatajudError::InvalidInput {
                message: format!(
                    "'{input}' contains {} digits; a CNJ process number has exactly {DIGITS}. \
                     Accepted formats: 'NNNNNNN-DD.AAAA.J.TR.OOOO' (e.g. '0003462-37.2022.4.05.8400') \
                     or 20 digits without punctuation",
                    digits.len()
                ),
            });
        }
        Ok(Self { digits })
    }

    /// The check digits carried by the number (positions 8 and 9).
    #[must_use]
    #[inline]
    pub fn given_check_digits(&self) -> String {
        self.digits.get(7..9).unwrap_or_default().to_owned()
    }

    /// The check digits recomputed from the number (Resolução CNJ 65/2008).
    ///
    /// Algorithm: take the 18 digits `NNNNNNNAAAATROOOO` (the number without
    /// the check digits), multiply by 100, and subtract the remainder modulo
    /// 97 from 98.
    #[must_use]
    #[inline]
    pub fn expected_check_digits(&self) -> Option<String> {
        let head = self.digits.get(0..7)?;
        let tail = self.digits.get(9..DIGITS)?;
        let base: u64 = format!("{head}{tail}").parse().ok()?;
        let remainder = base.checked_rem(97)?.checked_mul(100)?.checked_rem(97)?;
        let expected = 98_u64.checked_sub(remainder)?;
        Some(format!("{expected:02}"))
    }

    /// Whether the number's check digits are consistent with its other digits.
    #[must_use]
    #[inline]
    pub fn check_digits_valid(&self) -> bool {
        self.expected_check_digits()
            .is_some_and(|expected| expected == self.given_check_digits())
    }

    /// Infers the tribunal alias from the segment (`J`) and tribunal (`TR`)
    /// fields of the number, when the combination is known.
    ///
    /// Covered: `J=4` federal (`trf1`..`trf6`), `J=5` labor (`trt1`..`trt24`),
    /// `J=6` electoral (`tre-XX`) and `J=8` state (`tjXX`) courts.
    #[must_use]
    #[inline]
    pub fn tribunal_hint(&self) -> Option<&'static str> {
        let segment: u8 = self.digits.get(13..14)?.parse().ok()?;
        let tribunal_code: u8 = self.digits.get(14..16)?.parse().ok()?;
        let candidate = match segment {
            4 => format!("trf{tribunal_code}"),
            5 => format!("trt{tribunal_code}"),
            6 => format!("tre-{}", state_code(tribunal_code)?),
            8 => format!("tj{}", state_code(tribunal_code)?),
            _ => return None,
        };
        tribunals::find(&candidate).map(|tribunal| tribunal.alias)
    }
}

/// Maps an IBGE state code (as used in the `TR` field) to its lowercase
/// two-letter abbreviation. `53` (Distrito Federal) maps to `dft`, matching the
/// catalog aliases `tjdft` / `tre-dft`.
const fn state_code(code: u8) -> Option<&'static str> {
    match code {
        11 => Some("ro"),
        12 => Some("ac"),
        13 => Some("am"),
        14 => Some("rr"),
        15 => Some("pa"),
        16 => Some("ap"),
        17 => Some("to"),
        21 => Some("ma"),
        22 => Some("pi"),
        23 => Some("ce"),
        24 => Some("rn"),
        25 => Some("pb"),
        26 => Some("pe"),
        27 => Some("al"),
        28 => Some("se"),
        29 => Some("ba"),
        31 => Some("mg"),
        32 => Some("es"),
        33 => Some("rj"),
        35 => Some("sp"),
        41 => Some("pr"),
        42 => Some("sc"),
        43 => Some("rs"),
        50 => Some("ms"),
        51 => Some("mt"),
        52 => Some("go"),
        53 => Some("dft"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_formatted_and_raw_numbers() {
        let formatted = ProcessNumber::parse("0003462-37.2022.4.05.8400").unwrap();
        let raw = ProcessNumber::parse("00034623720224058400").unwrap();
        assert_eq!(
            formatted.digits, "00034623720224058400",
            "punctuation stripped"
        );
        assert_eq!(formatted, raw, "both forms normalize to the same number");
    }

    #[test]
    fn rejects_wrong_digit_count_with_clear_message() {
        let error = ProcessNumber::parse("0003462-37.2022.4.05").unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("16 digits"),
            "counted digits are reported: {message}"
        );
        assert!(
            message.contains("exactly 20"),
            "the expected size is stated: {message}"
        );
    }

    #[test]
    fn validates_check_digits_of_known_numbers() {
        // Official wiki example and the TRF5 process exercised in this repo.
        for numero in [
            "00008323520184013202",
            "07223914020178070001",
            "00034623720224058400",
        ] {
            let parsed = ProcessNumber::parse(numero).unwrap();
            assert!(
                parsed.check_digits_valid(),
                "{numero} must have valid check digits"
            );
        }
        let corrupted = ProcessNumber::parse("00008329920184013202").unwrap();
        assert!(
            !corrupted.check_digits_valid(),
            "corrupted check digits must be detected"
        );
        assert_eq!(
            corrupted.expected_check_digits().as_deref(),
            Some("35"),
            "the expected check digits are computed"
        );
    }

    #[test]
    fn infers_tribunal_from_number() {
        let cases = [
            ("00034623720224058400", Some("trf5")), // Justiça Federal, TRF5
            ("00008323520184013202", Some("trf1")), // Justiça Federal, TRF1
            ("00000010020205030000", Some("trt3")), // Justiça do Trabalho, TRT3
            ("00000010020208350000", Some("tjsp")), // Justiça Estadual, TJSP (IBGE 35)
            ("00000010020208260000", Some("tjpe")), // Justiça Estadual, TJPE (IBGE 26)
            ("00000010020206350000", Some("tre-sp")), // Justiça Eleitoral, TRE-SP
            ("00000010020200000000", None),         // unknown segment
        ];
        for (numero, expected) in cases {
            let parsed = ProcessNumber::parse(numero).unwrap();
            assert_eq!(
                parsed.tribunal_hint(),
                expected,
                "tribunal hint for {numero}"
            );
        }
    }
}
