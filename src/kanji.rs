use kanjidic_parser::kanjidic::Kanjidic;
use kanjidic_types::Character;
use lazy_static::lazy_static;
use wana_kana::to_hiragana::to_hiragana;

lazy_static! {
    static ref KANJIDIC: Kanjidic = {
        let xml = include_str!("../blobs/kanjidic2.xml");
        let start = xml.find("<kanjidic2>").unwrap();
        let skipped = std::str::from_utf8(&xml.as_bytes()[start..]).unwrap();
        Kanjidic::try_from(skipped).expect("couldn't parse kanjidic file")
    };
}

pub fn lookup_kanji(by: &char) -> Option<Character> {
    for entry in KANJIDIC.characters.iter() {
        if entry.literal == *by {
            return Some(entry.clone());
        }
    }
    None
}

pub fn lookup_kanji_readings(by: &char) -> Option<impl Iterator<Item = String>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kanji_lookups_work() {
        lookup_kanji(&'優').unwrap();
    }

    #[test]
    fn kanji_reading_lookups_work() {
        assert_eq!(
            lookup_kanji_readings(&'美').unwrap().next(),
            Some("うつく".into())
        );
    }
}
