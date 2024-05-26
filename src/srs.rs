use std::time::Duration;

const FSRS_CONSTANTS: [f32; 17] = [
    0.4, 0.6, 2.4, 5.8, 4.93, 0.94, 0.86, 0.01, 1.49, 0.14, 0.94, 2.18, 0.05, 0.34, 1.26, 0.29,
    2.61,
];
// seconds in a day
const DAY_SECS: f32 = 86400.0;

#[derive(Debug, Clone, Copy)]
pub enum Rating {
    Again,
    Hard,
    Good,
    Easy,
}

impl Rating {
    fn as_num(&self) -> f32 {
        match *self {
            Self::Again => 1.0,
            Self::Hard => 2.0,
            Self::Good => 3.0,
            Self::Easy => 4.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Memo {
    pub stability: f32,
    pub difficulty: f32,
}

impl Memo {
    pub fn new(rating: Rating) -> Self {
        Self {
            stability: match rating {
                Rating::Again => FSRS_CONSTANTS[0],
                Rating::Hard => FSRS_CONSTANTS[1],
                Rating::Good => FSRS_CONSTANTS[2],
                Rating::Easy => FSRS_CONSTANTS[3],
            },
            difficulty: calc_difficulty(rating, None),
        }
    }

    pub fn retrievability(&self, elapsed: Duration) -> f32 {
        (1.0 + (elapsed.as_secs_f32() / DAY_SECS) / (4.26316 * self.stability)).powf(-0.5)
    }

    pub fn next_review(&self, desired_retention: f32) -> Duration {
        Duration::from_secs_f32(
            4.26316 * self.stability * (desired_retention.powf(-2.0) - 1.0) * DAY_SECS,
        )
    }

    pub fn review(&mut self, rating: Rating, elapsed: Duration) {
        self.difficulty = calc_difficulty(rating, Some(self.difficulty));
        if matches!(rating, Rating::Again) {
            let mut new_stability = FSRS_CONSTANTS[11];
            new_stability *= self.difficulty.powf(-FSRS_CONSTANTS[12]);
            new_stability *= (self.stability + 1.0).powf(FSRS_CONSTANTS[13]) - 1.0;
            new_stability *= (FSRS_CONSTANTS[14] * (1.0 - self.retrievability(elapsed))).exp();
            self.stability = new_stability;
        } else {
            let mut new_stability = FSRS_CONSTANTS[8].exp();
            new_stability *= 11.0 - self.difficulty;
            new_stability *= self.stability.powf(-FSRS_CONSTANTS[9]);
            new_stability *=
                (FSRS_CONSTANTS[10] * (1.0 - self.retrievability(elapsed))).exp() - 1.0;
            new_stability *= match rating {
                Rating::Hard => FSRS_CONSTANTS[15],
                Rating::Easy => FSRS_CONSTANTS[16],
                _ => 1.0,
            };
            new_stability += 1.0;
            new_stability *= self.stability;
            self.stability = new_stability;
        };
    }
}

fn calc_difficulty(rating: Rating, prev: Option<f32>) -> f32 {
    match prev {
        None => FSRS_CONSTANTS[4] - (rating.as_num() - 3.0) * FSRS_CONSTANTS[5],
        Some(prev) => {
            // new difficulty
            let mut difficulty = prev - FSRS_CONSTANTS[6] * (rating.as_num() - 3.0);
            // mean reversal
            difficulty *= 1.0 - FSRS_CONSTANTS[7];
            difficulty += FSRS_CONSTANTS[7] * calc_difficulty(Rating::Good, None);
            difficulty
        }
    }
}
