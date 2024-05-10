use pyo3::conversion::FromPyObject;
use pyo3::prelude::*;
use tokio::task;
use tokio::sync::{mpsc, oneshot};
use std::iter;

#[derive(Debug, Clone, Copy)]
#[allow(unused)]
pub enum PartOfSpeech {
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
impl PartOfSpeech {
    pub fn as_str(self) -> &'static str {
        match self {
            PartOfSpeech::Adjective => "ADJ",
            PartOfSpeech::Adposition => "ADP",
            PartOfSpeech::Adverb => "ADV",
            PartOfSpeech::Auxiliary => "AUX",
            PartOfSpeech::CoordinatingConjunction => "CCONJ",
            PartOfSpeech::Determiner => "DET",
            PartOfSpeech::Interjection => "INTJ",
            PartOfSpeech::Noun => "NOUN",
            PartOfSpeech::Numeral => "NUM",
            PartOfSpeech::Particle => "PART",
            PartOfSpeech::Pronoun => "PRON",
            PartOfSpeech::ProperNoun => "PROPN",
            PartOfSpeech::Punctuation => "PUNCT",
            PartOfSpeech::SubordinatingConjunction => "SCONJ",
            PartOfSpeech::Symbol => "SYM",
            PartOfSpeech::Verb => "VERB",
            PartOfSpeech::Other => "X",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "ADJ" => Some(PartOfSpeech::Adjective),
            "ADP" => Some(PartOfSpeech::Adposition),
            "ADV" => Some(PartOfSpeech::Adverb),
            "AUX" => Some(PartOfSpeech::Auxiliary),
            "CCONJ" => Some(PartOfSpeech::CoordinatingConjunction),
            "DET" => Some(PartOfSpeech::Determiner),
            "INTJ" => Some(PartOfSpeech::Interjection),
            "NOUN" => Some(PartOfSpeech::Noun),
            "NUM" => Some(PartOfSpeech::Numeral),
            "PART" => Some(PartOfSpeech::Particle),
            "PRON" => Some(PartOfSpeech::Pronoun),
            "PROPN" => Some(PartOfSpeech::ProperNoun),
            "PUNCT" => Some(PartOfSpeech::Punctuation),
            "SCONJ" => Some(PartOfSpeech::SubordinatingConjunction),
            "SYM" => Some(PartOfSpeech::Symbol),
            "VERB" => Some(PartOfSpeech::Verb),
            "X" => Some(PartOfSpeech::Other),
            _ => None,
        }
    }

    pub fn is_open(self) -> bool {
        match self {
            PartOfSpeech::Adjective => true,
            PartOfSpeech::Adverb => true,
            PartOfSpeech::Interjection => true,
            PartOfSpeech::Noun => true,
            PartOfSpeech::ProperNoun => true,
            PartOfSpeech::Verb => true,
            _ => false,
        }
    }

    pub fn is_closed(self) -> bool {
        match self {
            PartOfSpeech::Adposition => true,
            PartOfSpeech::Auxiliary => true,
            PartOfSpeech::CoordinatingConjunction => true,
            PartOfSpeech::Determiner => true,
            PartOfSpeech::Numeral => true,
            PartOfSpeech::Particle => true,
            PartOfSpeech::Pronoun => true,
            PartOfSpeech::SubordinatingConjunction => true,
            _ => false,
        }
    }

    pub fn is_other(self) -> bool {
        match self {
            PartOfSpeech::Punctuation => true,
            PartOfSpeech::Symbol => true,
            PartOfSpeech::Other => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WordUnit {
    pub unit: String,
    pub class: PartOfSpeech,
}

impl WordUnit {
    pub fn lemmatize(&self) -> String {
        match self.class {
            PartOfSpeech::Verb => todo!(),
            PartOfSpeech::Adjective => todo!(),
            _ => self.unit.clone(),
        }
    }

    /// Attemps to find this word in the dictionary.
    /// If found, returns the jmdict entry and the matched dictionary form.
    pub fn lookup(&self) -> Option<(jmdict::Entry, &str)> {
        // Don't bother looking up a dictionary entry for punctuation, symbols, etc.
        if self.class.is_other() {
            return None;
        }

        let mut candidate: Option<(jmdict::Entry, &str)> = None;
        let mut lcp_len = 0usize;
        let readings = jmdict::entries()
            // TODO: maybe in some cases its still worth returning a match even if
            // can_be_candidate_for is false?
            .filter(|entry| entry.senses().any(|sense| sense.can_be_candidate_for(self.class)))
            .map(|entry| entry
                .kanji_elements()
                .map(|r| r.text)
                .chain(entry.reading_elements().map(|r| r.text))
                .map(move |reading| (entry, reading))
            )
            .flatten();

        for (entry, reading) in readings {
            // TODO: rely on lemmatization instead of longest common prefix?
            // or should lemmatization rely on the dictionary lookup? shrug
            for (i, (a,b)) in iter::zip(reading.chars(), self.unit.chars()).enumerate() {
                if a != b {
                    break;
                }
                if i+1 == lcp_len && lcp_len > 0 {
                    if let Some((entry, c,)) = candidate {
                        if reading.len() < c.len() {
                            candidate = Some((entry, reading,));
                        }
                    }
                } else if i+1 > lcp_len {
                    lcp_len = i+1;
                    candidate = Some((entry, reading,));
                }
            }
        }

        candidate
    }
}

#[derive(Debug, Clone)]
pub struct Morphology {
    pub units: Vec<WordUnit>,
    pub deps: Vec<usize>,
}

impl<'py> FromPyObject<'py> for Morphology {
    fn extract_bound(ob: &Bound<'py, PyAny>) -> PyResult<Self> {
        let ob = ob.clone().into_gil_ref().getattr("values")?;

        let parts: Vec<String> = ob.get_item(1)?.extract()?;
        let parts = parts.into_iter();

        let tags: Vec<String> = ob.get_item(3)?.extract()?;
        let tags = tags.into_iter().map(|tag| PartOfSpeech::from_str(&tag).unwrap());

        let units = std::iter::zip(parts, tags)
            .map(|(unit, class)| WordUnit { unit, class, })
            .collect();

        let deps: Vec<usize> = ob.get_item(6)?.extract()?;

        Ok(Self { units, deps, })
    }
}

type Request = (String, oneshot::Sender<Morphology>,);

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
                        Some((input, res_tx,)) => {
                            let morphology = nlp.getattr("analyze")?.call1((input,))?.extract()?;
                            res_tx.send(morphology).unwrap();
                        }
                        None => return Ok(()),
                    }
                }
            })
        });

        init_rx.await.unwrap();
        Self { _handle, tx, }
    }

    pub async fn analyze(&self, input: impl Into<String>) -> anyhow::Result<Morphology> {
        let (tx, rx) = oneshot::channel();
        self.tx.send((input.into(), tx))?;
        let morphology = rx.await?;
        Ok(morphology)
    }
}

trait JMDictSenseExt {
    fn can_be_candidate_for(&self, class: PartOfSpeech) -> bool;
}

impl JMDictSenseExt for jmdict::Sense {
    fn can_be_candidate_for(&self, class: PartOfSpeech) -> bool {
        self.parts_of_speech().any(|jmdict_pos| {
            match class {
                PartOfSpeech::Adjective => {
                    match jmdict_pos {
                        jmdict::PartOfSpeech::Adjective => true,
                        jmdict::PartOfSpeech::YoiAdjective => true,
                        jmdict::PartOfSpeech::AdjectivalNoun => true,
                        jmdict::PartOfSpeech::NoAdjective => true,
                        jmdict::PartOfSpeech::PreNounAdjectival => true,
                        jmdict::PartOfSpeech::TaruAdjective => true,
                        jmdict::PartOfSpeech::AuxiliaryAdjective => true,
                        jmdict::PartOfSpeech::Unclassified => true,
                        _ => false,
                    }
                },
                PartOfSpeech::Adposition => {
                    match jmdict_pos {
                        jmdict::PartOfSpeech::Particle => true,
                        _ => false,
                    }
                },
                PartOfSpeech::Adverb => {
                    match jmdict_pos {
                        jmdict::PartOfSpeech::Adverb => true,
                        jmdict::PartOfSpeech::AdverbTakingToParticle => true,
                        jmdict::PartOfSpeech::Particle => true,
                        jmdict::PartOfSpeech::Unclassified => true,
                        _ => false,
                    }
                },
                PartOfSpeech::Auxiliary => {
                    match jmdict_pos {
                        jmdict::PartOfSpeech::Auxiliary => true,
                        jmdict::PartOfSpeech::AuxiliaryAdjective => true,
                        jmdict::PartOfSpeech::AuxiliaryVerb => true,
                        jmdict::PartOfSpeech::SpecialSuruVerb => true,
                        _ => false,
                    }
                },
                PartOfSpeech::CoordinatingConjunction => {
                    match jmdict_pos {
                        jmdict::PartOfSpeech::Conjunction => true,
                        jmdict::PartOfSpeech::Particle => true,
                        _ => false,
                    }
                },
                PartOfSpeech::Determiner => {
                    match jmdict_pos {
                        jmdict::PartOfSpeech::Pronoun => true,
                        _ => false,
                    }
                },
                PartOfSpeech::Interjection => {
                    match jmdict_pos {
                        jmdict::PartOfSpeech::Expression => true,
                        jmdict::PartOfSpeech::Interjection => true,
                        _ => false,
                    }
                },
                PartOfSpeech::Noun => {
                    match jmdict_pos {
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
                    }
                },
                PartOfSpeech::Numeral => false,
                PartOfSpeech::Particle => {
                    match jmdict_pos {
                        jmdict::PartOfSpeech::Conjunction => true,
                        jmdict::PartOfSpeech::Suffix => true,
                        jmdict::PartOfSpeech::Prefix => true,
                        jmdict::PartOfSpeech::Particle => true,
                        _ => false,
                    }
                },
                PartOfSpeech::Pronoun => match jmdict_pos {
                    jmdict::PartOfSpeech::Pronoun => true,
                    _ => false,
                },
                PartOfSpeech::ProperNoun => match jmdict_pos {
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
                PartOfSpeech::Punctuation => false,
                PartOfSpeech::SubordinatingConjunction => {
                    match jmdict_pos {
                        jmdict::PartOfSpeech::Auxiliary => true,
                        jmdict::PartOfSpeech::AuxiliaryAdjective => true,
                        jmdict::PartOfSpeech::AuxiliaryVerb => true,
                        jmdict::PartOfSpeech::Conjunction => true,
                        jmdict::PartOfSpeech::Particle => true,
                        _ => false,
                    }
                },
                PartOfSpeech::Symbol => false,
                PartOfSpeech::Verb => match jmdict_pos {
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
                PartOfSpeech::Other => false,
            }
        })
    }
}

// documenting failures/oddities for heuristic crafting:
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