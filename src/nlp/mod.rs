use pyo3::conversion::FromPyObject;
use pyo3::prelude::*;
use std::iter;
use tokio::sync::{mpsc, oneshot};
use tokio::task;

// TODO: parameterize by categories. tense, politeness, polarity blah blah
#[derive(Debug, Clone, Copy)]
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
            match unit.lookup_exact() {
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

        match unit.class {
            UposTag::Adjective => Self::Adjective,
            UposTag::Adposition => Self::Particle,
            UposTag::Adverb => Self::Adverb,
            // TODO: check if auxiliaries are ever not verbs, if so impl disambiguate_auxiliary
            UposTag::Auxiliary => Self::Verb,
            UposTag::CoordinatingConjunction => disambiguate_conjunction(unit),
            UposTag::Determiner => Self::Determiner,
            UposTag::Interjection => Self::Expression,
            UposTag::Noun => Self::Noun,
            UposTag::Numeral => Self::Other,
            UposTag::Particle => Self::Particle,
            UposTag::Pronoun => Self::Pronoun,
            UposTag::ProperNoun => Self::Noun,
            UposTag::Punctuation => Self::Other,
            UposTag::SubordinatingConjunction => disambiguate_conjunction(unit),
            UposTag::Symbol => Self::Other,
            UposTag::Verb => Self::Verb,
            UposTag::Other => Self::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Word {
    pub text: String,
    pub role: WordRole,
    pub upos_subunits: Vec<WordUnit>, // TODO: handle inner dependencies correctly
}

impl std::fmt::Display for Word {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

impl Word {
    pub fn lookup(&self) -> Option<(jmdict::Entry, &str)> {
        self.upos_subunits[0].lookup()
    }
}

pub type Dependency = Option<usize>;

#[derive(Debug, Clone)]
pub struct Morphology {
    /// tuple of the word and the index to the dependency
    /// dependency is None if this is the root of the sentence
    units: Vec<(Word, Dependency)>,
}

impl Morphology {
    pub fn from_analysis(analysis: Analysis) -> Self {
        struct MergedUnit {
            role: WordRole,
            subunits: Vec<WordUnit>,
            dep_index: Option<usize>,
        }

        let mut merged: Vec<MergedUnit> = vec![];
        let mut association: Vec<usize> = vec![];

        for (_unit, _dep) in iter::zip(analysis.units, analysis.deps) {
            let i = merged.len().saturating_sub(1);
            if let Some(last) = merged.last_mut() {
                match _unit.class {
                    UposTag::Auxiliary | UposTag::SubordinatingConjunction => {
                        match last.subunits[0].class {
                            UposTag::Verb if _dep > 0 => {
                                if association[_dep - 1] == i {
                                    last.subunits.push(_unit);
                                    continue;
                                }
                            }
                            UposTag::Adjective if _dep > 0 => (), // TODO
                            _ => (),
                        };
                        // TODO: handling of aru verb, which gets treated as an AUX
                    }
                    _ => (),
                }
            }
            merged.push(MergedUnit {
                role: WordRole::from_upos(&_unit),
                subunits: vec![_unit],
                dep_index: match _dep {
                    0 => None,
                    i => Some(i - 1),
                },
            });
            association.push(merged.len() - 1);
        }

        Self {
            units: merged
                .into_iter()
                .map(
                    |MergedUnit {
                         role,
                         subunits,
                         dep_index,
                     }| {
                        (
                            Word {
                                role,
                                text: subunits.iter().map(|u| u.unit.as_str()).collect(),
                                upos_subunits: subunits.into_iter().collect(),
                            },
                            dep_index,
                        )
                    },
                )
                .collect(),
        }
    }

    pub fn word(&self, index: usize) -> &Word {
        &self.units[index].0
    }

    pub fn get_word(&self, index: usize) -> Option<&Word> {
        self.units.get(index).map(|v| &v.0)
    }

    pub fn words(&self) -> impl Iterator<Item = &Word> {
        self.units.iter().map(|v| &v.0)
    }

    pub fn dependency(&self, index: usize) -> Dependency {
        self.units[index].1
    }

    pub fn get_dependency(&self, index: usize) -> Option<Dependency> {
        self.units.get(index).map(|v| v.1)
    }

    pub fn dependencies<'a>(&'a self) -> impl Iterator<Item = Dependency> + 'a {
        self.units.iter().map(|v| v.1)
    }
}

#[derive(Debug, Clone, Copy)]
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
impl UposTag {
    pub fn as_str(self) -> &'static str {
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

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "ADJ" => Some(UposTag::Adjective),
            "ADP" => Some(UposTag::Adposition),
            "ADV" => Some(UposTag::Adverb),
            "AUX" => Some(UposTag::Auxiliary),
            "CCONJ" => Some(UposTag::CoordinatingConjunction),
            "DET" => Some(UposTag::Determiner),
            "INTJ" => Some(UposTag::Interjection),
            "NOUN" => Some(UposTag::Noun),
            "NUM" => Some(UposTag::Numeral),
            "PART" => Some(UposTag::Particle),
            "PRON" => Some(UposTag::Pronoun),
            "PROPN" => Some(UposTag::ProperNoun),
            "PUNCT" => Some(UposTag::Punctuation),
            "SCONJ" => Some(UposTag::SubordinatingConjunction),
            "SYM" => Some(UposTag::Symbol),
            "VERB" => Some(UposTag::Verb),
            "X" => Some(UposTag::Other),
            _ => None,
        }
    }

    pub fn is_open(self) -> bool {
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

    pub fn is_closed(self) -> bool {
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

    pub fn is_other(self) -> bool {
        match self {
            UposTag::Punctuation => true,
            UposTag::Symbol => true,
            UposTag::Other => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WordUnit {
    pub unit: String,
    pub class: UposTag,
}

impl WordUnit {
    // TODO: implement lemmatization by undoing inflection
    pub fn lemmatize(&self) -> String {
        match self.class {
            UposTag::Verb => todo!(),
            UposTag::Adjective => todo!(),
            _ => self.unit.clone(),
        }
    }

    /// Attemps to find this word in the dictionary.
    /// If found, returns the jmdict entry and the matched dictionary form.
    pub fn lookup(&self) -> Option<(jmdict::Entry, &str)> {
        match self.class {
            // TODO: rely on lemmatization instead of longest common prefix
            UposTag::Verb | UposTag::Adjective => self.lookup_partial(),
            // Don't bother looking up a dictionary entry for punctuation, symbols, etc.
            _ if self.class.is_other() => None,
            // Exact lookup for words which won't be found inflected.
            _ => self.lookup_exact(),
        }
    }

    /// Least common prefix based dictionary lookup.
    fn lookup_partial(&self) -> Option<(jmdict::Entry, &str)> {
        let mut candidate: Option<(jmdict::Entry, &str)> = None;
        let mut lcp_len = 0usize;
        let readings = jmdict::entries()
            // maybe in some cases its still worth returning a match even if
            // can_be_candidate_for is false?
            .filter(|entry| {
                entry
                    .senses()
                    .any(|sense| sense.can_be_candidate_for(self.class))
            })
            .map(|entry| {
                entry
                    .kanji_elements()
                    .map(|r| r.text)
                    .chain(entry.reading_elements().map(|r| r.text))
                    .map(move |reading| (entry, reading))
            })
            .flatten();

        for (entry, reading) in readings {
            for (i, (a, b)) in iter::zip(reading.chars(), self.unit.chars()).enumerate() {
                if a != b {
                    break;
                }
                if i + 1 == lcp_len && lcp_len > 0 {
                    if let Some((entry, c)) = candidate {
                        if reading.len() < c.len() {
                            candidate = Some((entry, reading));
                        }
                    }
                } else if i + 1 > lcp_len {
                    lcp_len = i + 1;
                    candidate = Some((entry, reading));
                }
            }
        }

        candidate
    }

    // TODO: index the dictionary for random access
    // TODO: DRY?
    fn lookup_exact(&self) -> Option<(jmdict::Entry, &str)> {
        jmdict::entries()
            .filter(|entry| {
                entry
                    .senses()
                    .any(|sense| sense.can_be_candidate_for(self.class))
            })
            .map(|entry| {
                entry
                    .kanji_elements()
                    .map(|r| r.text)
                    .chain(entry.reading_elements().map(|r| r.text))
                    .map(move |reading| (entry, reading))
            })
            .flatten()
            .find(|(_, reading)| *reading == &self.unit)
    }
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub units: Vec<WordUnit>,
    pub deps: Vec<usize>,
}

impl<'py> FromPyObject<'py> for Analysis {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let ob = ob.clone().into_gil_ref().getattr("values")?;

        let parts: Vec<String> = ob.get_item(1)?.extract()?;
        let parts = parts.into_iter();

        let tags: Vec<String> = ob.get_item(3)?.extract()?;
        let tags = tags.into_iter().map(|tag| UposTag::from_str(&tag).unwrap());

        let units = std::iter::zip(parts, tags)
            .map(|(unit, class)| WordUnit { unit, class })
            .collect();

        let deps: Vec<usize> = ob.get_item(6)?.extract()?;

        Ok(Self { units, deps })
    }
}

type Request = (String, oneshot::Sender<Analysis>);

pub struct Engine {
    _handle: task::JoinHandle<anyhow::Result<()>>,
    tx: mpsc::UnboundedSender<Request>,
}

impl Engine {
    // TODO: error handling
    pub async fn init() -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<Request>();
        let (init_tx, init_rx) = oneshot::channel();

        let _handle = task::spawn_blocking(move || {
            Python::with_gil(|py| {
                let nlp = PyModule::from_code_bound(py, include_str!("nlp.py"), "nlp.py", "nlp")?;
                init_tx.send(()).unwrap();
                loop {
                    match rx.blocking_recv() {
                        Some((input, res_tx)) => {
                            let morphology = nlp.getattr("analyze")?.call1((input,))?.extract()?;
                            res_tx.send(morphology).unwrap();
                        }
                        None => return Ok(()),
                    }
                }
            })
        });

        init_rx.await.unwrap();
        Self { _handle, tx }
    }

    pub async fn morphological_analysis(
        &self,
        input: impl Into<String>,
    ) -> anyhow::Result<Analysis> {
        let (tx, rx) = oneshot::channel();
        self.tx.send((input.into(), tx))?;
        let morphology = rx.await?;
        Ok(morphology)
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
                    jmdict::PartOfSpeech::Particle => true,
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
