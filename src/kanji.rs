use lazy_static::lazy_static;
use kanjidic_parser::kanjidic::Kanjidic;
use kanjidic_types::Character;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kanji_lookups_work() {
        lookup_kanji(&'優').unwrap();
    }
}