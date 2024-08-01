use std::collections::HashMap;

use lazy_static::lazy_static;

lazy_static! {
    pub static ref INDEX_BY_READING: HashMap<String, Vec<jmdict::Entry>> = {
        let mut map = HashMap::new();

        for entry in jmdict::entries() {
            let readings = entry
                .kanji_elements()
                .map(|el| el.text)
                .chain(entry.reading_elements().map(|el| el.text));
            for reading in readings {
                map.entry(reading.into()).or_insert(vec![]).push(entry);
            }
        }
        map
    };
}
