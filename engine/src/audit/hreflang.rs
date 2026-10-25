//! Validating hreflang language tags.
//!
//! The common mistakes are not syntax errors but plausible-looking wrong
//! codes — `en-UK` instead of `en-GB`, `pt-PT` written `pt_PT`, a language
//! where a region belongs. Catching those needs the actual ISO code lists, so
//! they are embedded below rather than approximated with a pattern.
//!
//! Values are matched case-insensitively, because the HTML spec says hreflang
//! is case-insensitive: `en-GB` and `en-gb` are both valid.

use rusqlite::Connection;

use crate::error::Result;

/// ISO 639-1 two-letter language codes.
const LANGUAGES: &str = "aa ab ae af ak am an ar as av ay az ba be bg bh bi bm bn bo br bs ca ce \
    ch co cr cs cu cv cy da de dv dz ee el en eo es et eu fa ff fi fj fo fr fy ga gd gl gn gu gv \
    ha he hi ho hr ht hu hy hz ia id ie ig ii ik io is it iu ja jv ka kg ki kj kk kl km kn ko kr \
    ks ku kv kw ky la lb lg li ln lo lt lu lv mg mh mi mk ml mn mr ms mt my na nb nd ne ng nl nn \
    no nr nv ny oc oj om or os pa pi pl ps pt qu rm rn ro ru rw sa sc sd se sg si sk sl sm sn so \
    sq sr ss st su sv sw ta te tg th ti tk tl tn to tr ts tt tw ty ug uk ur uz ve vi vo wa wo xh \
    yi yo za zh zu";

/// ISO 3166-1 alpha-2 region codes, plus `UK` is deliberately *absent*: it is
/// the single most common invalid hreflang region, and the point is to catch it.
const REGIONS: &str = "AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ \
    BL BM BN BO BQ BR BS BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ \
    DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP \
    GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI \
    KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP \
    MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM \
    PN PR PS PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX \
    SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU \
    WF WS YE YT ZA ZM ZW";

/// Is this an ISO 639-1 language code?
pub(crate) fn is_known_language(code: &str) -> bool {
    LANGUAGES
        .split(' ')
        .any(|known| known.eq_ignore_ascii_case(code.trim()))
}

/// Is this a usable hreflang value?
pub fn is_valid(raw: &str) -> bool {
    let value = raw.trim();
    if value.is_empty() {
        return false;
    }
    // The one permitted non-language value: the fallback for unmatched users.
    if value.eq_ignore_ascii_case("x-default") {
        return true;
    }

    let mut parts = value.split('-');
    let Some(language) = parts.next() else {
        return false;
    };
    if !LANGUAGES
        .split(' ')
        .any(|code| code.eq_ignore_ascii_case(language))
    {
        return false;
    }

    match parts.next() {
        None => true,
        Some(region) => {
            // A UN M.49 numeric region ("es-419" for Latin America) is valid.
            let numeric = region.len() == 3 && region.chars().all(|c| c.is_ascii_digit());
            let iso = REGIONS
                .split(' ')
                .any(|code| code.eq_ignore_ascii_case(region));
            // Nothing may follow the region.
            (numeric || iso) && parts.next().is_none()
        }
    }
}

/// Explain why a value was rejected, for the issue's detail column.
pub fn describe(raw: &str) -> String {
    let value = raw.trim();
    if value.is_empty() {
        return "empty hreflang value".to_string();
    }
    if value.contains('_') {
        return format!("`{value}` uses an underscore; hreflang uses a hyphen");
    }
    let language = value.split('-').next().unwrap_or(value);
    if !LANGUAGES
        .split(' ')
        .any(|code| code.eq_ignore_ascii_case(language))
    {
        return format!("`{value}` is not a known language code");
    }
    if value.eq_ignore_ascii_case("en-uk") {
        return "`en-UK` is not valid; the country code for the UK is `GB`".to_string();
    }
    format!("`{value}` has an unrecognised region code")
}

/// Pages declaring an hreflang value that is not a valid language tag.
pub fn invalid_codes(conn: &Connection) -> Result<Vec<(i64, Option<String>)>> {
    let mut stmt = conn.prepare("SELECT url_id, lang FROM hreflang ORDER BY url_id")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut findings: Vec<(i64, Option<String>)> = Vec::new();
    for (url_id, lang) in rows {
        if !is_valid(&lang) {
            // One finding per page; the detail names the first bad value.
            if findings.last().map(|(id, _)| *id) != Some(url_id) {
                findings.push((url_id, Some(describe(&lang))));
            }
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_languages_and_regions_are_valid() {
        for value in [
            "en", "fr", "de", "pt", "zh", "en-GB", "pt-BR", "zh-CN", "es-419",
        ] {
            assert!(is_valid(value), "{value} should be valid");
        }
    }

    #[test]
    fn case_does_not_matter() {
        // The HTML spec says hreflang is case-insensitive.
        for value in ["EN", "en-gb", "En-Gb", "X-Default"] {
            assert!(is_valid(value), "{value} should be valid");
        }
    }

    #[test]
    fn x_default_is_the_only_special_value() {
        assert!(is_valid("x-default"));
        assert!(!is_valid("x-whatever"));
        assert!(!is_valid("default"));
    }

    #[test]
    fn the_common_real_world_mistakes_are_caught() {
        // A country code that does not exist, though it looks obvious.
        assert!(!is_valid("en-UK"));
        assert!(describe("en-UK").contains("GB"));
        // The wrong separator.
        assert!(!is_valid("en_US"));
        assert!(describe("en_US").contains("hyphen"));
        // A language code that is not ISO 639-1.
        assert!(!is_valid("eng"));
        assert!(
            !is_valid("uk-en"),
            "region and language the wrong way round"
        );
        // Too many parts.
        assert!(!is_valid("en-GB-oxendict"));
        assert!(!is_valid(""));
        assert!(!is_valid("-"));
    }

    #[test]
    fn describe_is_specific_about_the_problem() {
        assert!(describe("").contains("empty"));
        assert!(describe("zz").contains("not a known language"));
        assert!(describe("en-ZZ").contains("region"));
    }
}
