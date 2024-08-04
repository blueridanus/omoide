use pyo3::prelude::*;

pub mod args;
pub mod dedup;
pub mod dict;
pub mod document;
pub mod kanji;
pub mod nlp;
pub mod srs;
pub mod subs;

#[pymodule]
fn omoide(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<nlp::WordUnit>()?;
    m.add_class::<nlp::WordRole>()?;
    m.add_class::<nlp::Word>()?;
    m.add_class::<nlp::Morphology>()?;
    m.add_class::<nlp::Analysis>()?;
    m.add_class::<nlp::DocumentTokenization>()?;
    m.add_class::<nlp::UposTag>()?;
    Ok(())
}
