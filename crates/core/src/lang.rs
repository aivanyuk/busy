//! The languages busy's windows are translated into. The strings themselves are `busy_ui::i18n`.

use serde::{Deserialize, Deserializer, Serialize};

/// A UI language, stored in the config file by its BCP 47 tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lang {
    #[serde(rename = "en")]
    En,
    #[serde(rename = "de")]
    De,
    #[serde(rename = "es")]
    Es,
    #[serde(rename = "fr")]
    Fr,
    #[serde(rename = "it")]
    It,
    #[serde(rename = "ja")]
    Ja,
    #[serde(rename = "ko")]
    Ko,
    #[serde(rename = "pl")]
    Pl,
    #[serde(rename = "pt-BR")]
    PtBr,
    #[serde(rename = "ru")]
    Ru,
    #[serde(rename = "zh-Hans")]
    ZhHans,
}

impl Lang {
    pub const ALL: [Lang; 11] = [
        Lang::En,
        Lang::De,
        Lang::Es,
        Lang::Fr,
        Lang::It,
        Lang::Ja,
        Lang::Ko,
        Lang::Pl,
        Lang::PtBr,
        Lang::Ru,
        Lang::ZhHans,
    ];

    /// Position in `Lang::ALL`.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// BCP 47 tag, also the locale name DirectWrite and the package's resources take.
    pub fn tag(self) -> &'static str {
        match self {
            Lang::En => "en-US",
            Lang::De => "de-DE",
            Lang::Es => "es-ES",
            Lang::Fr => "fr-FR",
            Lang::It => "it-IT",
            Lang::Ja => "ja-JP",
            Lang::Ko => "ko-KR",
            Lang::Pl => "pl-PL",
            Lang::PtBr => "pt-BR",
            Lang::Ru => "ru-RU",
            Lang::ZhHans => "zh-CN",
        }
    }

    /// The language's name in itself, as a language picker lists it.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::De => "Deutsch",
            Lang::Es => "Español",
            Lang::Fr => "Français",
            Lang::It => "Italiano",
            Lang::Ja => "日本語",
            Lang::Ko => "한국어",
            Lang::Pl => "Polski",
            Lang::PtBr => "Português (Brasil)",
            Lang::Ru => "Русский",
            Lang::ZhHans => "中文(简体)",
        }
    }

    /// The language busy has for one BCP 47 tag ("de-AT", "zh-Hans-CN", "pt-PT"), matched by its primary
    /// subtag. Chinese only in its simplified script: a traditional-script tag (zh-TW, zh-HK, zh-MO, zh-Hant)
    /// has no match rather than Simplified Chinese.
    pub fn from_tag(tag: &str) -> Option<Lang> {
        let tag = tag.to_ascii_lowercase();
        let mut parts = tag.split(['-', '_']);
        let lang = match parts.next()? {
            "en" => Lang::En,
            "de" => Lang::De,
            "es" => Lang::Es,
            "fr" => Lang::Fr,
            "it" => Lang::It,
            "ja" => Lang::Ja,
            "ko" => Lang::Ko,
            "pl" => Lang::Pl,
            "pt" => Lang::PtBr,
            "ru" => Lang::Ru,
            "zh" if !parts.any(|p| matches!(p, "hant" | "tw" | "hk" | "mo")) => Lang::ZhHans,
            _ => return None,
        };
        Some(lang)
    }

    /// The first of the user's preferred UI languages, most preferred first, that busy has; else English.
    pub fn pick<'a>(tags: impl IntoIterator<Item = &'a str>) -> Lang {
        tags.into_iter().find_map(Lang::from_tag).unwrap_or(Lang::En)
    }
}

/// `Config::language`: a language this build doesn't have (written by a newer busy, or a hand-edit) is the
/// system's, instead of failing the whole file.
pub(crate) fn lenient<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Lang>, D::Error> {
    Ok(serde_json::Value::deserialize(d).ok().and_then(|v| serde_json::from_value(v).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags() {
        assert_eq!(Lang::from_tag("de-AT"), Some(Lang::De));
        assert_eq!(Lang::from_tag("PT-pt"), Some(Lang::PtBr));
        assert_eq!(Lang::from_tag("zh-Hans-CN"), Some(Lang::ZhHans));
        assert_eq!(Lang::from_tag("zh-CN"), Some(Lang::ZhHans));
        assert_eq!(Lang::from_tag("zh-TW"), None);
        assert_eq!(Lang::from_tag("zh-Hant-HK"), None);
        assert_eq!(Lang::from_tag("nl-NL"), None);
        assert_eq!(Lang::from_tag(""), None);
        for l in Lang::ALL {
            assert_eq!(Lang::from_tag(l.tag()), Some(l));
            assert_eq!(Lang::ALL[l.index()], l);
        }
    }

    #[test]
    fn picks_first_supported() {
        assert_eq!(Lang::pick(["nl-NL", "fr-CA", "en-US"]), Lang::Fr);
        assert_eq!(Lang::pick(["zh-TW", "ja-JP"]), Lang::Ja);
        assert_eq!(Lang::pick(["nl-NL"]), Lang::En);
        assert_eq!(Lang::pick([]), Lang::En);
    }
}
