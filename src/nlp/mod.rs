use pyo3::conversion::FromPyObject;
use pyo3::prelude::*;
use std::iter;
use tokio::sync::{mpsc, oneshot};
use tokio::task;

use crate::dict::INDEX_BY_READING;
use crate::kanji::KANJI_RE;

// TODO: parameterize by categories. tense, politeness, polarity blah blah
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[pyclass]
pub enum WordRole {
    Verb,
    Noun,
    Adjective,
    Adverb,
    Pronoun,
    Determiner,
    Particle,
    Conjunction,
    Counter,
    Copula,
    Expression,
    Other,
}

impl WordRole {
    /// Best effort to convert a upos tagged subword into one of our word classes.
    /// This one uses no context from surrounding units, that's done by `Morphology::from_analysis`.
    fn from_upos(unit: &WordUnit) -> Self {
        fn disambiguate_conjunction(unit: &WordUnit) -> WordRole {
            match unit.lookup_with_pos_filter().next() {
                // heuristic: if this word can be a particle, it's a particle
                // TODO: disambiguation between semes by AGI?
                Some((entry, _)) => match entry.senses().any(|s| {
                    s.parts_of_speech()
                        .any(|pos| matches!(pos, jmdict::PartOfSpeech::Particle))
                }) {
                    true => WordRole::Particle,
                    false => WordRole::Conjunction,
                },
                None => WordRole::Conjunction,
            }
        }

        fn disambiguate_verb(unit: &WordUnit) -> WordRole {
            if matches!(unit.lemma.as_str(), "だ" | "です") {
                WordRole::Copula
            } else {
                WordRole::Verb
            }
        }

        // onomatopoeia or tokenization errors
        if matches!(
            unit.lemma.as_str(),
            "お" | "あ" | "ん" | "あっ" | "はっ" | "ううん" | "うん" | "うう" | "えっ" | "う" | "い"
        ) {
            return Self::Other;
        }

        match unit.class {
            UposTag::Adjective => Self::Adjective,
            UposTag::Adposition => Self::Particle,
            UposTag::Adverb => Self::Adverb,
            UposTag::Auxiliary => disambiguate_verb(unit),
            UposTag::CoordinatingConjunction => disambiguate_conjunction(unit),
            UposTag::Determiner => Self::Determiner,
            UposTag::Interjection => Self::Expression,
            // TODO: counters
            UposTag::Noun => Self::Noun,
            UposTag::Numeral => Self::Other,
            UposTag::Particle => Self::Particle,
            UposTag::Pronoun => Self::Pronoun,
            UposTag::ProperNoun => Self::Noun,
            UposTag::Punctuation => Self::Other,
            UposTag::SubordinatingConjunction => disambiguate_conjunction(unit),
            UposTag::Symbol => Self::Other,
            UposTag::Verb => disambiguate_verb(unit),
            UposTag::Other => Self::Other,
        }
    }

    pub fn is_open(&self) -> bool {
        match self {
            WordRole::Verb => true,
            WordRole::Noun => true,
            WordRole::Adjective => true,
            WordRole::Adverb => true,
            WordRole::Pronoun => false,
            WordRole::Determiner => false,
            WordRole::Particle => false,
            WordRole::Conjunction => false,
            WordRole::Counter => true,
            WordRole::Copula => false,
            WordRole::Expression => true,
            WordRole::Other => false,
        }
    }
}

#[derive(Debug, Clone)]
#[pyclass]
pub struct Word {
    pub text: String,
    pub lemma_units: Vec<WordUnit>,
    pub role: WordRole,
    pub upos_subunits: Vec<WordUnit>, // TODO: handle inner dependencies correctly
}

impl std::fmt::Display for Word {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

#[pymethods]
impl Word {
    pub fn lemma(&self) -> String {
        self.lemma_units.iter().map(|u| u.lemma.as_str()).collect()
    }

    pub fn has_kanji(&self) -> bool {
        KANJI_RE.is_match(self.text.as_str())
    }

    fn __str__(&self) -> &str {
        &self.text
    }
}

impl Word {
    pub fn lookup(&self, lookup_closed: bool) -> Option<(jmdict::Entry, String)> {
        for n in (1..=self.lemma_units.len()).rev() {
            let merged_reading = self
                .lemma_units
                .iter()
                .take(n)
                .map(|t| t.lemma.as_str())
                .collect::<String>();
            if let Some(entries) = INDEX_BY_READING.get(&merged_reading) {
                if !lookup_closed && !self.role.is_open() {
                    return None;
                }

                let entry = entries.iter().find(|entry| {
                    entry
                        .senses()
                        .any(|sense| sense.can_be_candidate_for(self.lemma_units[0].class))
                });

                if let Some(entry) = entry {
                    return Some((entry.clone(), merged_reading));
                } else {
                    return Some((entries[0], merged_reading));
                }
            }
        }

        return None;
    }
}

pub type Dependency = usize;

#[derive(Debug, Clone)]
#[pyclass]
pub struct Morphology {
    /// tuple of the word and the index to the dependency
    /// dependency is None if this is the root of the sentence
    units: Vec<(Word, Dependency)>,
}

#[pymethods]
impl Morphology {
    #[new]
    pub fn from_analysis(analysis: Analysis) -> Self {
        struct MergedUnit {
            lemma_units: Vec<WordUnit>,
            role: WordRole,
            subunits: Vec<WordUnit>,
            i: usize,
            dep_i: usize,
        }

        let mut merged: Vec<MergedUnit> = vec![];
        let mut mapping: Vec<usize> = vec![];

        for (i, (_unit, _dep)) in iter::zip(analysis.units, analysis.deps).enumerate() {
            let role = WordRole::from_upos(&_unit);
            if let Some(last) = merged.last_mut() {
                // merge inflections into the word
                let mut is_inflection = false;

                if matches!(
                    _unit.class,
                    UposTag::Auxiliary | UposTag::SubordinatingConjunction
                ) {
                    is_inflection = true;
                }

                if matches!(_unit.lemma.as_str(), "た") && matches!(_unit.class, UposTag::Verb) {
                    is_inflection = true;
                }

                if matches!(_unit.lemma.as_str(), "です" | "だ") {
                    is_inflection = false;
                }

                if is_inflection {
                    last.subunits.push(_unit);
                    mapping.push(merged.len() - 1);
                    continue;
                }

                // try to merge nouns if compound present in dictionary
                if last.role == role && matches!(role, WordRole::Noun) {
                    let merged_reading = last
                        .subunits
                        .iter()
                        .map(|t| t.unit.as_str())
                        .chain([_unit.lemma.as_str()])
                        .collect::<String>();
                    if let Some(entries) = INDEX_BY_READING.get(&merged_reading) {
                        let entry = entries.iter().find(|entry| {
                            entry
                                .senses()
                                .any(|sense| sense.can_be_candidate_for(_unit.class))
                        });

                        if entry.is_some() {
                            last.lemma_units.push(_unit.clone());
                            last.subunits.push(_unit);
                            mapping.push(merged.len() - 1);
                            continue;
                        }
                    }
                }
            }
            mapping.push(merged.len().saturating_sub(1));
            merged.push(MergedUnit {
                lemma_units: vec![_unit.clone()],
                role,
                subunits: vec![_unit],
                i,
                dep_i: _dep,
            });
        }

        Self {
            units: merged
                .into_iter()
                .map(
                    |MergedUnit {
                         role,
                         subunits,
                         lemma_units,
                         i,
                         dep_i,
                     }| {
                        (
                            Word {
                                lemma_units,
                                role,
                                text: subunits.iter().map(|u| u.unit.as_str()).collect(),
                                upos_subunits: subunits.into_iter().collect(),
                            },
                            mapping[dep_i],
                        )
                    },
                )
                .collect(),
        }
    }

    pub fn dependency(&self, index: usize) -> Dependency {
        self.units[index].1
    }

    pub fn get_dependency(&self, index: usize) -> Option<Dependency> {
        self.units.get(index).map(|v| v.1)
    }

    pub fn __getitem__(&self, index: usize) -> Option<Word> {
        self.units.get(index).map(|v| v.0.clone())
    }

    #[pyo3(name = "words")]
    fn words_py(&self) -> Vec<Word> {
        self.units.iter().cloned().map(|v| v.0).collect()
    }
}

impl Morphology {
    pub fn words(&self) -> impl Iterator<Item = &Word> {
        self.units.iter().map(|v| &v.0)
    }

    pub fn dependencies<'a>(&'a self) -> impl Iterator<Item = Dependency> + 'a {
        self.units.iter().map(|v| v.1)
    }

    pub fn word(&self, index: usize) -> &Word {
        &self.units[index].0
    }

    pub fn get_word(&self, index: usize) -> Option<&Word> {
        self.units.get(index).map(|v| &v.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[pyclass]
pub enum UposTag {
    Adjective,
    Adposition,
    Adverb,
    Auxiliary,
    CoordinatingConjunction,
    Determiner,
    Interjection,
    Noun,
    Numeral,
    Particle,
    Pronoun,
    ProperNoun,
    Punctuation,
    SubordinatingConjunction,
    Symbol,
    Verb,
    Other,
}

// TODO: consider a more specialized set of categories for our purposes which we
// can convert to from the upos output. see comment at the end of this file
#[pymethods]
impl UposTag {
    pub fn as_str(&self) -> &'static str {
        match self {
            UposTag::Adjective => "ADJ",
            UposTag::Adposition => "ADP",
            UposTag::Adverb => "ADV",
            UposTag::Auxiliary => "AUX",
            UposTag::CoordinatingConjunction => "CCONJ",
            UposTag::Determiner => "DET",
            UposTag::Interjection => "INTJ",
            UposTag::Noun => "NOUN",
            UposTag::Numeral => "NUM",
            UposTag::Particle => "PART",
            UposTag::Pronoun => "PRON",
            UposTag::ProperNoun => "PROPN",
            UposTag::Punctuation => "PUNCT",
            UposTag::SubordinatingConjunction => "SCONJ",
            UposTag::Symbol => "SYM",
            UposTag::Verb => "VERB",
            UposTag::Other => "X",
        }
    }

    #[new]
    pub fn from_str(s: &str) -> Self {
        match s {
            "ADJ" => UposTag::Adjective,
            "ADP" => UposTag::Adposition,
            "ADV" => UposTag::Adverb,
            "AUX" => UposTag::Auxiliary,
            "CCONJ" => UposTag::CoordinatingConjunction,
            "DET" => UposTag::Determiner,
            "INTJ" => UposTag::Interjection,
            "NOUN" => UposTag::Noun,
            "NUM" => UposTag::Numeral,
            "PART" => UposTag::Particle,
            "PRON" => UposTag::Pronoun,
            "PROPN" => UposTag::ProperNoun,
            "PUNCT" => UposTag::Punctuation,
            "SCONJ" => UposTag::SubordinatingConjunction,
            "SYM" => UposTag::Symbol,
            "VERB" => UposTag::Verb,
            _ => UposTag::Other,
        }
    }

    pub fn is_open(&self) -> bool {
        match self {
            UposTag::Adjective => true,
            UposTag::Adverb => true,
            UposTag::Interjection => true,
            UposTag::Noun => true,
            UposTag::ProperNoun => true,
            UposTag::Verb => true,
            _ => false,
        }
    }

    pub fn is_closed(&self) -> bool {
        match self {
            UposTag::Adposition => true,
            UposTag::Auxiliary => true,
            UposTag::CoordinatingConjunction => true,
            UposTag::Determiner => true,
            UposTag::Numeral => true,
            UposTag::Particle => true,
            UposTag::Pronoun => true,
            UposTag::SubordinatingConjunction => true,
            _ => false,
        }
    }

    pub fn is_other(&self) -> bool {
        match self {
            UposTag::Punctuation => true,
            UposTag::Symbol => true,
            UposTag::Other => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
#[pyclass]
pub struct WordUnit {
    pub unit: String,
    pub lemma: String,
    pub class: UposTag,
}

// TODO: implement lemmatization by undoing inflection
#[pymethods]
impl WordUnit {
    #[new]
    fn py_new(unit: String, lemma: String, class: UposTag) -> Self {
        Self { unit, lemma, class }
    }

    fn __str__(&self) -> &str {
        &self.unit
    }

    pub fn lemmatize(&self) -> String {
        match self.class {
            UposTag::Verb => todo!(),
            UposTag::Adjective => todo!(),
            _ => self.unit.clone(),
        }
    }

    pub fn has_kanji(&self) -> bool {
        KANJI_RE.is_match(self.unit.as_str())
    }
}

impl WordUnit {
    /// Attemps to find this word in the dictionary.
    /// If found, returns the jmdict entry and the matched dictionary form.
    pub fn lookup(&self, lookup_closed: bool) -> Option<(&jmdict::Entry, &str)> {
        if self.class.is_open() || lookup_closed {
            let found = self.lookup_with_pos_filter().next();
            if found.is_some() {
                return found;
            } else {
                return self.lookup_by_readings().next();
            }
        } else {
            return None;
        }
    }

    // TODO: index the dictionary for random access
    // TODO: DRY?
    fn lookup_with_pos_filter(&self) -> impl Iterator<Item = (&jmdict::Entry, &str)> {
        self.lookup_by_readings().filter(|(entry, _)| {
            entry
                .senses()
                .any(|sense| sense.can_be_candidate_for(self.class))
        })
    }

    fn lookup_by_readings(&self) -> impl Iterator<Item = (&jmdict::Entry, &str)> {
        let (reading, entries) = match crate::dict::INDEX_BY_READING.get_key_value(&self.lemma) {
            Some((reading, entries)) => (reading.as_str(), entries.as_slice()),
            None => ("", Default::default()),
        };
        entries.iter().map(move |e| (e, reading))
    }
}

#[derive(Debug, Clone)]
#[pyclass]
pub struct Analysis {
    pub units: Vec<WordUnit>,
    pub deps: Vec<usize>,
}

impl From<AnalysisRaw> for Analysis {
    fn from(value: AnalysisRaw) -> Self {
        Self {
            units: value.units,
            deps: value.deps,
        }
    }
}

#[pymethods]
impl Analysis {
    #[new]
    fn py_new(units: Vec<WordUnit>, deps: Vec<usize>) -> Self {
        Self { units, deps }
    }
}

#[derive(Debug, Clone)]
pub struct AnalysisRaw {
    pub units: Vec<WordUnit>,
    pub deps: Vec<usize>,
}

impl<'py> FromPyObject<'py> for AnalysisRaw {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let ob = ob.clone();

        let parts: Vec<String> = ob.get_item(0)?.extract()?;
        let parts = parts.into_iter();

        let tags: Vec<String> = ob.get_item(1)?.extract()?;
        let tags = tags.into_iter().map(|tag| UposTag::from_str(&tag));

        let lemmas: Vec<String> = ob.get_item(2)?.extract()?;
        let lemmas = lemmas.into_iter();

        use std::iter::zip;

        let units = zip(zip(parts, lemmas), tags)
            .map(|((unit, lemma), class)| WordUnit { unit, lemma, class })
            .collect();

        let deps: Vec<usize> = ob.get_item(3)?.extract()?;

        Ok(Self { units, deps })
    }
}

pub struct Engine {
    _handle: task::JoinHandle<()>,
    tx: mpsc::UnboundedSender<EngineCommand>,
}

enum EngineCommand {
    Analyze(Vec<String>, oneshot::Sender<Vec<Analysis>>),
    Tokenize(Vec<String>, oneshot::Sender<DocumentTokenization>),
}

#[derive(Clone, Debug)]
#[pyclass]
pub struct DocumentTokenization {
    pub tokenization: Vec<Vec<String>>, // TODO: holy allocations...? those strings are very small
}

impl Engine {
    // TODO: error handling
    pub async fn init() -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<EngineCommand>();
        let (init_tx, init_rx) = oneshot::channel();

        let _handle = task::spawn_blocking(move || {
            let done: anyhow::Result<()> = Python::with_gil(|py| {
                let nlp = PyModule::from_code_bound(py, include_str!("nlp.py"), "nlp.py", "nlp")?;
                init_tx.send(()).unwrap();
                loop {
                    if let Some(cmd) = rx.blocking_recv() {
                        match cmd {
                            EngineCommand::Analyze(input, res_tx) => {
                                let morphologies: Vec<AnalysisRaw> =
                                    nlp.getattr("analyze")?.call1((input,)).unwrap().extract()?;

                                res_tx
                                    .send(morphologies.into_iter().map(|v| v.into()).collect())
                                    .unwrap();
                            }
                            EngineCommand::Tokenize(input, res_tx) => {
                                let tokenization = nlp
                                    .getattr("tokenize")?
                                    .call1((input,))
                                    .unwrap()
                                    .extract()?;
                                res_tx.send(DocumentTokenization { tokenization }).unwrap();
                            }
                        }
                    } else {
                        return Ok(());
                    }
                }
            });
            done.unwrap()
        });

        init_rx.await.unwrap();
        Self { _handle, tx }
    }

    pub async fn morphological_analysis(&self, input: String) -> anyhow::Result<Analysis> {
        let mut morphologies = self
            .morphological_analysis_batch(vec![input.into()])
            .await?;

        Ok(morphologies.pop().unwrap())
    }

    pub async fn morphological_analysis_batch(
        &self,
        input: Vec<String>,
    ) -> anyhow::Result<Vec<Analysis>> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(EngineCommand::Analyze(input.into(), tx))?;
        let morphologies = rx.await?;
        Ok(morphologies)
    }

    pub async fn tokenize_batch(&self, input: Vec<String>) -> anyhow::Result<DocumentTokenization> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(EngineCommand::Tokenize(input, tx))?;
        let tokenized = rx.await?;
        Ok(tokenized)
    }
}

trait JMDictSenseExt {
    fn can_be_candidate_for(&self, class: UposTag) -> bool;
}

impl JMDictSenseExt for jmdict::Sense {
    fn can_be_candidate_for(&self, class: UposTag) -> bool {
        self.parts_of_speech().any(|jmdict_pos| {
            match class {
                UposTag::Adjective => match jmdict_pos {
                    jmdict::PartOfSpeech::Adjective => true,
                    jmdict::PartOfSpeech::YoiAdjective => true,
                    jmdict::PartOfSpeech::AdjectivalNoun => true,
                    jmdict::PartOfSpeech::NoAdjective => true,
                    jmdict::PartOfSpeech::PreNounAdjectival => true,
                    jmdict::PartOfSpeech::TaruAdjective => true,
                    jmdict::PartOfSpeech::AuxiliaryAdjective => true,
                    jmdict::PartOfSpeech::Unclassified => true,
                    _ => false,
                },
                UposTag::Adposition => match jmdict_pos {
                    jmdict::PartOfSpeech::Particle => true,
                    _ => false,
                },
                UposTag::Adverb => match jmdict_pos {
                    jmdict::PartOfSpeech::Adverb => true,
                    jmdict::PartOfSpeech::AdverbTakingToParticle => true,
                    jmdict::PartOfSpeech::Unclassified => true,
                    _ => false,
                },
                UposTag::Auxiliary => match jmdict_pos {
                    jmdict::PartOfSpeech::Auxiliary => true,
                    jmdict::PartOfSpeech::AuxiliaryAdjective => true,
                    jmdict::PartOfSpeech::AuxiliaryVerb => true,
                    jmdict::PartOfSpeech::SpecialSuruVerb => true,
                    _ => false,
                },
                // TODO: check if this is too strict
                UposTag::CoordinatingConjunction => match jmdict_pos {
                    jmdict::PartOfSpeech::Conjunction => true,
                    jmdict::PartOfSpeech::Particle => true,
                    _ => false,
                },
                UposTag::Determiner => match jmdict_pos {
                    jmdict::PartOfSpeech::Pronoun => true,
                    _ => false,
                },
                UposTag::Interjection => match jmdict_pos {
                    jmdict::PartOfSpeech::Expression => true,
                    jmdict::PartOfSpeech::Interjection => true,
                    _ => false,
                },
                UposTag::Noun => match jmdict_pos {
                    jmdict::PartOfSpeech::NounOrVerbActingPrenominally => true,
                    jmdict::PartOfSpeech::AdjectivalNoun => true,
                    jmdict::PartOfSpeech::PreNounAdjectival => true,
                    jmdict::PartOfSpeech::Counter => true,
                    jmdict::PartOfSpeech::CommonNoun => true,
                    jmdict::PartOfSpeech::AdverbialNoun => true,
                    jmdict::PartOfSpeech::ProperNoun => true,
                    jmdict::PartOfSpeech::NounPrefix => true,
                    jmdict::PartOfSpeech::NounSuffix => true,
                    jmdict::PartOfSpeech::Suffix => true,
                    jmdict::PartOfSpeech::TemporalNoun => true,
                    jmdict::PartOfSpeech::Unclassified => true,
                    _ => false,
                },
                UposTag::Numeral => false,
                UposTag::Particle => match jmdict_pos {
                    jmdict::PartOfSpeech::Conjunction => true,
                    jmdict::PartOfSpeech::Suffix => true,
                    jmdict::PartOfSpeech::Prefix => true,
                    jmdict::PartOfSpeech::Particle => true,
                    _ => false,
                },
                UposTag::Pronoun => match jmdict_pos {
                    jmdict::PartOfSpeech::Pronoun => true,
                    _ => false,
                },
                UposTag::ProperNoun => match jmdict_pos {
                    jmdict::PartOfSpeech::NounOrVerbActingPrenominally => true,
                    jmdict::PartOfSpeech::AdjectivalNoun => true,
                    jmdict::PartOfSpeech::PreNounAdjectival => true,
                    jmdict::PartOfSpeech::Counter => true,
                    jmdict::PartOfSpeech::CommonNoun => true,
                    jmdict::PartOfSpeech::AdverbialNoun => true,
                    jmdict::PartOfSpeech::ProperNoun => true,
                    jmdict::PartOfSpeech::NounPrefix => true,
                    jmdict::PartOfSpeech::NounSuffix => true,
                    jmdict::PartOfSpeech::Suffix => true,
                    jmdict::PartOfSpeech::TemporalNoun => true,
                    jmdict::PartOfSpeech::Unclassified => true,
                    _ => false,
                },
                UposTag::Punctuation => false,
                UposTag::SubordinatingConjunction => match jmdict_pos {
                    jmdict::PartOfSpeech::Auxiliary => true,
                    jmdict::PartOfSpeech::AuxiliaryAdjective => true,
                    jmdict::PartOfSpeech::AuxiliaryVerb => true,
                    jmdict::PartOfSpeech::Conjunction => true,
                    jmdict::PartOfSpeech::Particle => true,
                    _ => false,
                },
                UposTag::Symbol => false,
                UposTag::Verb => match jmdict_pos {
                    jmdict::PartOfSpeech::NounOrVerbActingPrenominally => true,
                    jmdict::PartOfSpeech::Auxiliary => true,
                    jmdict::PartOfSpeech::AuxiliaryVerb => true,
                    jmdict::PartOfSpeech::Unclassified => true,
                    jmdict::PartOfSpeech::UnspecifiedVerb => true,
                    jmdict::PartOfSpeech::IchidanVerb => true,
                    jmdict::PartOfSpeech::IchidanKureruVerb => true,
                    jmdict::PartOfSpeech::GodanAruVerb => true,
                    jmdict::PartOfSpeech::GodanBuVerb => true,
                    jmdict::PartOfSpeech::GodanGuVerb => true,
                    jmdict::PartOfSpeech::GodanKuVerb => true,
                    jmdict::PartOfSpeech::GodanIkuVerb => true,
                    jmdict::PartOfSpeech::GodanMuVerb => true,
                    jmdict::PartOfSpeech::GodanNuVerb => true,
                    jmdict::PartOfSpeech::GodanRuVerb => true,
                    jmdict::PartOfSpeech::IrregularGodanRuVerb => true,
                    jmdict::PartOfSpeech::GodanSuVerb => true,
                    jmdict::PartOfSpeech::GodanTsuVerb => true,
                    jmdict::PartOfSpeech::GodanUVerb => true,
                    jmdict::PartOfSpeech::IrregularGodanUVerb => true,
                    jmdict::PartOfSpeech::IntransitiveVerb => true,
                    jmdict::PartOfSpeech::KuruVerb => true,
                    jmdict::PartOfSpeech::IrregularGodanNuVerb => true,
                    jmdict::PartOfSpeech::IrregularGodanRuVerbWithPlainRiForm => true,
                    jmdict::PartOfSpeech::SuruVerb => true,
                    jmdict::PartOfSpeech::SuruPrecursorVerb => true,
                    jmdict::PartOfSpeech::IncludedSuruVerb => true,
                    jmdict::PartOfSpeech::SpecialSuruVerb => true,
                    jmdict::PartOfSpeech::TransitiveVerb => true,
                    jmdict::PartOfSpeech::IchidanZuruVerb => true,
                    _ => false,
                },
                UposTag::Other => false,
            }
        })
    }
}

// documenting failures/oddities for heuristic crafting and tests:
//
// #0. でもね、夢が見られない。
//    separates でもね into で[CCONJ] も[ADP] ね[PART]
//    should have been でも[ADV] ね[PART]
// => suggested heuristic: closed form classes should be merged into the longest dictionary
// => word made up of them, iff the units are not verb inflections, i.e. don't trigger if
// => that で was obviously te-form for a verb
//
// #1. 犬を１匹と猫を２匹飼っています。
//    ..１匹[NOUN]..２匹[ADV] ..
//    the counters got merged into the number and treated as a noun and adverb respectively
//    since we do least common prefix for the dictionary lookup we won't find the counter word
// => suggested heuristic: start by stripping numbers from the string to be looked up so we
// => can at least find the counter. going forward we can also consider our own set of PoS
// => classes specialized for what we care about (e.g. including a counter class)
//
// #2. お宅様からいただいたお菓子は大変おいしゅうございました
//    お宅様 gets split into NOUN, NOUN, NOUN, the first two have a dependency on the third and
//    the dep is classed as 'compound' i.e. the whole thing is being understood as a compound noun
//    お菓子 gets treated the same way, split into お and 菓子
// => suggested heuristic: honorific/humble language might need special casing
// => also we don't currently keep dep classes, looks like they can come in handy
//
// #3. 赤くないボールを取ってください。
//     The JMDict glosses for the verb (取る) give:
//     1. to lose an easy game
//     2. to suffer an unexpected defeat
//     3. to lose information
//     WHY???
//
// #4. 鑑識課の知り合いから
//    treatment of compounds again, 鑑識課 getting split as two words 鑑識 課
// => suggested heuristic: merge open forms belonging to the same class if merged entry
//    is found in dictionary
//
// #5. あと、いつでもトイレに行けます
//    行けます gets lemmatized to 行ける, which is still not dictionary form - that would be 行く
// => might need to implement an algorithm to undo inflection instead of relying on spacy lemmatization
