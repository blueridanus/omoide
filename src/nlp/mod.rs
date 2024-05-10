use pyo3::conversion::FromPyObject;
use pyo3::prelude::*;
use tokio::task;
use tokio::sync::{mpsc, oneshot};

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
            PartOfSpeech::Verb => {
                todo!()
            },
            // TODO: lemmatization for other part of speech categories as well
            // off the top of my head that means adjectives, but need to think if
            // other classes inflect too
            _ => self.unit.clone(),
        }
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