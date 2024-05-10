mod srs;
mod nlp;

use std::time::Duration;
use srs::*;

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
    let morphology = nlp_engine.analyze("太郎は花子が読んでいる本を次郎に渡します").await?;

    for (i, token) in morphology.units.iter().enumerate() {
        let entry = jmdict::entries().find(|e| {
            e.kanji_elements().any(|k| k.text == token.unit)
        });
        
        println!(
            "{}: {:?}, {}",
            token.unit,
            token.class,
            match morphology.deps[i] {
                0 => "root".into(),
                dep_i => format!("depends on {}", morphology.units[dep_i-1].unit),
            },
        );
        
        // match entry {
        //     // doesnt properly find words yet, because you need to do lemmatization
        //     Some(found) => println!("Found entry in JMDict: {}", found.reading_elements().next().unwrap().text),
        //     None => println!("No match found in JMDict"),
        // }
    }
    Ok(())
}
