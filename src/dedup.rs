use std::{collections::HashMap, iter, ops::BitXor};

use crate::{
    document::Document,
    nlp::{DocumentTokenization, Engine},
};

pub fn doc_minhash(input: &DocumentTokenization) -> Vec<u64> {
    let hashers: Vec<u64> = (42..242).map(|v| fxhash::hash64(&v)).collect();
    let mut minhashes = Vec::with_capacity(200);

    let input: Vec<String> = input.tokenization.iter().cloned().flatten().collect();

    for j in 0..200 {
        let mut minhash = u64::MAX;
        for i in 1..input.len() - 1 {
            let hash = fxhash::hash64(&format!("{}{}{}", input[i - 1], input[i], input[i + 1]));
            minhash = hash.bitxor(hashers[j]).min(minhash);
        }
        minhashes.push(minhash);
    }

    minhashes
}

pub fn minhash_jaccard_similarity(a: &[u64], b: &[u64]) -> f32 {
    assert_eq!(a.len(), b.len());

    let mut matched = 0u32;
    for (a, b) in iter::zip(a, b) {
        if a == b {
            matched += 1;
        }
    }

    matched as f32 / a.len() as f32
}

pub struct DocumentDedupSet {
    _docs: Vec<(Document, Vec<u64>)>,
    doc_map: HashMap<(u32, u64), Vec<usize>>,
}

impl DocumentDedupSet {
    pub fn new() -> Self {
        Self {
            _docs: vec![],
            doc_map: HashMap::new(),
        }
    }

    fn insert_inner(&mut self, doc: Document, minhashes: Vec<u64>) -> Option<usize> {
        let bands: Vec<u64> = minhashes.as_slice().chunks(4).map(fxhash::hash64).collect();
        for (band_i, band_hash) in bands.iter().enumerate() {
            if let Some(candidates) = self.doc_map.get(&(band_i as u32, *band_hash)) {
                for candidate in candidates {
                    if minhash_jaccard_similarity(&self._docs[*candidate].1, &minhashes) > 0.8 {
                        return None;
                    }
                }
            }
        }

        for (i, band) in bands.iter().enumerate() {
            self.doc_map
                .entry((i as u32, *band))
                .or_insert(vec![])
                .push(self._docs.len());
        }

        self._docs.push((doc, minhashes));

        Some(self._docs.len() - 1)
    }

    pub async fn insert(
        &mut self,
        engine: &Engine,
        mut doc: Document,
    ) -> anyhow::Result<Option<usize>> {
        doc.tokenize(engine).await?;
        Ok(self.insert_tokenized(doc))
    }

    pub fn insert_tokenized(&mut self, doc: Document) -> Option<usize> {
        let minhashes = doc_minhash(doc.tokenization().expect("document must be tokenized"));
        self.insert_inner(doc, minhashes)
    }

    pub fn docs(&self) -> impl Iterator<Item = &Document> {
        self._docs.iter().map(|d| &d.0)
    }

    pub fn into_docs(self) -> impl Iterator<Item = Document> {
        self._docs.into_iter().map(|d| d.0)
    }
}

impl std::ops::Index<usize> for DocumentDedupSet {
    type Output = Document;

    fn index(&self, index: usize) -> &Self::Output {
        &self._docs[index].0
    }
}

impl std::ops::IndexMut<usize> for DocumentDedupSet {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self._docs[index].0
    }
}
