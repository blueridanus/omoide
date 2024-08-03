use std::path::{Path, PathBuf};

use crate::{
    nlp::{Analysis, DocumentTokenization, Engine},
    subs::SubtitleChunk,
};

pub struct Document {
    _chunks: Vec<DocumentChunk>,
    _source: Option<PathBuf>,
    _tokenization: Option<DocumentTokenization>,
    _analysis: Option<Vec<Analysis>>,
}

pub enum DocumentChunk {
    Plaintext(String),
    Subs(SubtitleChunk),
}

impl DocumentChunk {
    pub fn contents(&self) -> &str {
        match self {
            DocumentChunk::Plaintext(c) => c.as_str(),
            DocumentChunk::Subs(c) => c.content.as_str(),
        }
    }
}

impl From<SubtitleChunk> for DocumentChunk {
    fn from(value: SubtitleChunk) -> Self {
        Self::Subs(value)
    }
}

impl Document {
    pub fn new(contents: Vec<DocumentChunk>) -> Self {
        Self {
            _chunks: contents,
            _source: None,
            _tokenization: None,
            _analysis: None,
        }
    }

    pub fn new_with_source(contents: Vec<DocumentChunk>, source: PathBuf) -> Self {
        Self {
            _chunks: contents,
            _source: Some(source),
            _tokenization: None,
            _analysis: None,
        }
    }

    pub fn chunks(&self) -> &[DocumentChunk] {
        self._chunks.as_slice()
    }

    pub fn contents(&self) -> impl Iterator<Item = &str> {
        self._chunks.iter().map(DocumentChunk::contents)
    }

    pub fn tokenization(&self) -> Option<&DocumentTokenization> {
        self._tokenization.as_ref()
    }

    pub async fn tokenize(&mut self, engine: &Engine) -> anyhow::Result<&DocumentTokenization> {
        if self._tokenization.is_none() {
            let sentences = self.contents().map(String::from).collect();
            self._tokenization = Some(engine.tokenize_batch(sentences).await?);
        }
        Ok(self.tokenization().unwrap())
    }

    pub fn analysis(&self) -> Option<&[Analysis]> {
        self._analysis.as_ref().map(Vec::as_slice)
    }

    pub async fn analyze(&mut self, engine: &Engine) -> anyhow::Result<&[Analysis]> {
        if self._analysis.is_none() {
            let sentences = self.contents().map(String::from).collect();
            self._analysis = Some(engine.morphological_analysis_batch(sentences).await?);
        }
        Ok(self.analysis().unwrap())
    }

    pub fn source(&self) -> Option<&Path> {
        self._source.as_ref().map(PathBuf::as_path)
    }
}
