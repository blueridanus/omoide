use kanjidic_parser::kanjidic::Kanjidic;
use kanjidic_types::Character;
use lazy_static::lazy_static;
use regex::Regex;
use wana_kana::to_hiragana::to_hiragana;
use pyo3::prelude::*;

use crate::nlp::WordUnit;

lazy_static! {
    static ref KANJIDIC: Kanjidic = {
        let xml = include_str!("../blobs/kanjidic2.xml");
        let start = xml.find("<kanjidic2>").unwrap();
        let skipped = std::str::from_utf8(&xml.as_bytes()[start..]).unwrap();
        Kanjidic::try_from(skipped).expect("couldn't parse kanjidic file")
    };
    pub(crate) static ref KANJI_RE: Regex = Regex::new(r"\p{Han}+").unwrap();
}

pub fn lookup_kanji(by: char) -> Option<Character> {
    for entry in KANJIDIC.characters.iter() {
        if entry.literal == by {
            return Some(entry.clone());
        }
    }
    None
}

pub fn lookup_kanji_readings(by: char) -> Option<impl Iterator<Item = String>> {
    if let Some(kanji) = lookup_kanji(by) {
        use kanjidic_types::Reading::*;
        let mut readings: Vec<String> = kanji
            .readings
            .into_iter()
            .filter_map(|r| match r {
                Onyomi(s) => Some(s),
                Kunyomi(s) => Some(s.reading),
                _ => None,
            })
            .collect();
        readings.sort_by(|a, b| b.len().cmp(&a.len()));
        Some(readings.into_iter().map(|r| to_hiragana(r.as_str())))
    } else {
        None
    }
}

// TODO: should this be on morphology instead of analysis?
#[pymethods]
impl WordUnit {
    pub fn ruby_furigana(&self) -> Option<String> {
        let mut reading = None;
        if self.has_kanji() {
            if let Some((e, _)) = self.lookup(true) {
                reading = e.reading_elements().next();
            }
        }
        if reading.is_none() {
            return None;
        }
        let reading = reading.unwrap().text;
        let mut markup = String::new();
        let mut stack = &self.unit[..];
        let mut stack_r = &reading[..];
        let mut kanjiwise_markup = String::new();
        while let Some(re_match) = KANJI_RE.find(stack) {
            let skipped_chars = &stack[..re_match.start()].chars().count();
            stack = &stack[re_match.start()..];
            let (stack_r_skip, _) = stack_r.char_indices().skip(*skipped_chars).next().unwrap();
            stack_r = &stack_r[stack_r_skip..];
            let kanji = stack.chars().next().unwrap();
            kanjiwise_markup.push(kanji);

            if let Some(mut kanji_readings) = lookup_kanji_readings(kanji) {
                if let Some(matched) = kanji_readings.find(|r| stack_r.starts_with(r)) {
                    stack = &stack[kanji.len_utf8()..];
                    stack_r = &stack_r[matched.len()..];
                    kanjiwise_markup.push_str("<rp>(</rp><rt>");
                    kanjiwise_markup.push_str(&matched);
                    kanjiwise_markup.push_str("</rt><rp>)</rp>");
                    continue;
                }
            }

            kanjiwise_markup.clear();
            break;
        }

        markup.push_str("<ruby>");
        if !kanjiwise_markup.is_empty() {
            markup.push_str(&kanjiwise_markup);
            markup.push_str(stack_r); // remaining kana
        } else {
            markup.push_str(&self.unit);
            // we failed to align the furigana to individual kanji, so use a simpler style
            markup.push_str("<rp>(</rp><rt>");
            markup.push_str(&reading);
            markup.push_str("</rt><rp>)</rp>");
        }

        markup.push_str("</ruby>");
        Some(markup)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kanji_lookups_work() {
        lookup_kanji('優').unwrap();
    }

    #[test]
    fn kanji_reading_lookups_work() {
        assert_eq!(
            lookup_kanji_readings('美').unwrap().next(),
            Some("うつく".into())
        );
    }
}
