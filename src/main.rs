use omoide::{
    nlp::{self, WordRole},
    srs::{Memo, Rating},
};
use std::time::Duration;

fn inspect(memo: &Memo) {
    let secs = memo.next_review(0.9).as_secs();
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    println!("next review in {} days {} hrs {} mins", days, hours, mins);
    println!("{:?}", memo);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    let nlp_engine = nlp::Engine::init().await;
    let analysis = nlp_engine
        .morphological_analysis("赤くないボールを取ってください。")
        .await?;
    let morphology = nlp::Morphology::from_analysis(analysis);

    for (i, word) in morphology.words().enumerate() {
        let candidate = word.lookup();
        println!(
            "{}: {:?}{}",
            word,
            word.role, //正直言って私はクラシック音楽が好きじゃない。かたや、モリーの方が完全にはまっている。
            match morphology.dependency(i) {
                None => ", root".into(),
                Some(dep_i) => match word.role {
                    WordRole::Other => "".into(),
                    _ => format!(", depends on {}", morphology.word(dep_i)),
                },
            },
        );
        if let Some(candidate) = candidate {
            println!("  best JMdict match: {:?}", candidate.1);

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
                println!("  {}. {}", i + 1, gloss.text);
            }
            println!()
        }
    }
    Ok(())
}
