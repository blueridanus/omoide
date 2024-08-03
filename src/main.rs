use clap::Parser;
use omoide::{
    args::*,
    dedup::DocumentDedupSet,
    document::{Document, DocumentChunk},
    nlp::{self, Analysis, WordRole},
    srs::{Memo, Rating},
    subs::{parse_subtitle_file, SubtitleChunk},
};
use std::time::Duration;
use std::{collections::HashMap, fs};
use std::{
    iter,
    path::{Path, PathBuf},
    usize,
};

fn inspect(memo: &Memo) {
    let secs = memo.next_review(0.9).as_secs();
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    println!("next review in {} days {} hrs {} mins", days, hours, mins);
    println!("{:?}", memo);
}

pub async fn process_sentences(sentences: Vec<String>) -> anyhow::Result<()> {
    let nlp_engine = nlp::Engine::init().await;
    let analyses = nlp_engine.morphological_analysis_batch(sentences).await?;
    for analysis in analyses {
        let text: String = analysis
            .units
            .iter()
            .map(|unit| unit.unit.as_str())
            .collect();
        println!("\nAnalysis for: {text}");
        let morphology = nlp::Morphology::from_analysis(analysis);

        for (i, word) in morphology.words().enumerate() {
            let candidate = word.lookup();
            println!(
                "- {}: {:?}{}",
                word,
                word.role, //正直言って私はクラシック音楽が好きじゃない。かたや、モリーの方が完全にはまっている。
                match morphology.dependency(i) {
                    dep_i if dep_i == i => ", root".into(),
                    dep_i => match word.role {
                        WordRole::Other => "".into(),
                        _ => format!(", depends on {}", morphology.word(dep_i)),
                    },
                },
            );
            if let Some(candidate) = candidate {
                println!("    best JMdict match: {:?}", candidate.1);

                for (i, gloss) in candidate
                    .0
                    .senses()
                    .map(|sense| {
                        sense.glosses().filter(|gloss| match gloss.gloss_type {
                            jmdict::GlossType::LiteralTranslation
                            | jmdict::GlossType::RegularTranslation => true,
                            _ => false,
                        })
                    })
                    .flatten()
                    .enumerate()
                {
                    println!("    {}. {}", i + 1, gloss.text);
                }
            }
        }
    }
    Ok(())
}

pub async fn practice() -> anyhow::Result<()> {
    // initial review: good
    let mut memo = Memo::new(Rating::Good);
    inspect(&memo);
    // easy review after 2 days
    memo.review(Rating::Easy, Duration::from_secs(86400 * 2));
    inspect(&memo);
    // hard review after another 2 days
    memo.review(Rating::Hard, Duration::from_secs(86400 * 2));
    inspect(&memo);
    // good review after another 2 days
    memo.review(Rating::Good, Duration::from_secs(86400 * 2));
    inspect(&memo);
    // oh no, tried reviewing it 4 days later and totally forgot it, oops
    memo.review(Rating::Again, Duration::from_secs(86400 * 4));
    // trying again after 60s, got it right
    memo.review(Rating::Good, Duration::from_secs(60));
    inspect(&memo);

    process_sentences(vec!["赤くないボールを取ってください。".into()]).await?;
    Ok(())
}

pub async fn manage(args: &ManageArgs) -> anyhow::Result<()> {
    if args.download {
        println!("I should download some subtitles");
    }
    Ok(())
}

type AnalyzedSubs = HashMap<PathBuf, Vec<(SubtitleChunk, Analysis)>>;
pub async fn retrieve_and_analyze_subs(subtitles_dir: &Path) -> anyhow::Result<DocumentDedupSet> {
    if subtitles_dir.exists() {
        let nlp_engine = nlp::Engine::init().await;

        let mut docs = DocumentDedupSet::new();

        for entry in fs::read_dir(subtitles_dir)?.filter_map(|x| x.ok()) {
            if entry.file_type()?.is_file() {
                let parsed = parse_subtitle_file(entry.path());
                match parsed {
                    Ok(content) => {
                        let doc = Document::new_with_source(
                            content.into_iter().map(|v| v.into()).collect(),
                            entry.path(),
                        );

                        if let Some(idx) = docs.insert(&nlp_engine, doc).await? {
                            println!("Processing: {}", entry.file_name().to_string_lossy());
                            docs[idx].analyze(&nlp_engine).await?;
                        } else {
                            println!(
                                "Skipping as duplicate: {}",
                                entry.file_name().to_string_lossy()
                            );
                        }
                    }
                    Err(e) => {
                        anyhow::bail!("Error in {}:\n{}", entry.path().display(), e);
                    }
                };
            }
        }

        println!();

        Ok(docs)
    } else {
        anyhow::bail!("Directory not found");
    }
}

pub async fn stats(args: &StatsArgs) -> anyhow::Result<()> {
    let mut occurrences: HashMap<String, usize> = HashMap::new();

    if args.subtitles_dir.exists() {
        let analyzed = retrieve_and_analyze_subs(&args.subtitles_dir)
            .await?
            .into_docs();

        for doc in analyzed {
            for analyzed_sentence in doc.analysis().unwrap() {
                for token in &analyzed_sentence.units {
                    if token.class.is_open() && token.lookup().is_some() {
                        *occurrences.entry(token.lemma.clone()).or_insert(0) += 1;
                    }
                }
            }
        }

        let mut occurrences: Vec<(String, usize)> = occurrences.into_iter().collect();
        occurrences.sort_by(|a, b| b.1.cmp(&a.1));

        println!("Top 25 words:");
        for (word, count) in occurrences.iter().take(25) {
            println!("  {word}: {count}");
        }
    }
    Ok(())
}

pub async fn analyse(args: AnalysisArgs) -> anyhow::Result<()> {
    let sentences = match args.srt_file {
        Some(srt_file) => crate::parse_subtitle_file(srt_file)?
            .into_iter()
            .map(|chunk| chunk.content)
            .collect(),
        None => args.sentence,
    };

    process_sentences(sentences).await
}

pub async fn examples(args: ExampleArgs) -> anyhow::Result<()> {
    let analyzed = retrieve_and_analyze_subs(&args.subtitles_dir)
        .await?
        .into_docs();
    let mut found = 0usize;

    for doc in analyzed {
        let mut found_in_file = false;
        for (analyzed_sentence, chunk) in iter::zip(doc.analysis().unwrap(), doc.chunks()) {
            if analyzed_sentence
                .units
                .iter()
                .find(|word| word.lemma == args.word)
                .is_some()
            {
                if !found_in_file {
                    println!(
                        "Found in {}:",
                        doc.source().unwrap().file_name().unwrap().to_string_lossy()
                    );
                    found_in_file = true;
                }
                found += 1;
                if let DocumentChunk::Subs(sub) = chunk {
                    println!(
                        "  [{}] {}",
                        format!(
                            "{:02}m{:02}s",
                            sub.start.as_secs() / 60,
                            sub.start.as_secs() % 60
                        ),
                        sub.content
                    );
                }
            }
            if let Some(max) = args.max {
                if found >= max {
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Some(Commands::Practice) | None => practice().await,
        Some(Commands::Manage(args)) => manage(&args).await,
        Some(Commands::Stats(args)) => stats(&args).await,
        Some(Commands::Analyse(args)) => analyse(args).await,
        Some(Commands::Examples(args)) => examples(args).await,
    }
}
