//! The others heard twice: with speakers rather than headphones, the microphone hears what the
//! computer plays, so the others' words come back as yours. Such a line of yours is left out
//! when it overlaps a line of theirs and, while it was said, the computer's sound was the louder
//! (what the microphone picked up came from the speakers), or its words are theirs.

use screengrabcraft_media::captions::Cue;

use crate::audio::FRAME_US;

/// Level steps the computer's sound is looked at either side (the speakers reach the microphone
/// a little late).
const LAG: usize = 5;
/// Below this the microphone heard nothing.
const VOICED_DB: f32 = -55.0;
/// The computer's sound this much louder than the microphone: what it heard was the speakers.
const CLEARLY_DB: f32 = 6.0;

fn words(text: &str) -> Vec<String> {
    text.to_lowercase().split(|c: char| !(c.is_alphanumeric() || c == '\'')).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// How many of `mine`'s words are among `theirs`, 0 to 1.
fn shared(mine: &str, theirs: &str) -> f32 {
    let (a, b) = (words(mine), words(theirs));
    if a.is_empty() {
        return 0.0;
    }
    a.iter().filter(|w| b.contains(w)).count() as f32 / a.len() as f32
}

/// The share of the moments the microphone heard something in `start..end` (µs) when the
/// computer's sound was louder.
fn louder_share(start: u64, end: u64, mic: &[f32], system: &[f32]) -> f32 {
    let (a, b) = ((start / FRAME_US) as usize, (end.div_ceil(FRAME_US) as usize).min(mic.len()));
    let (mut voiced, mut louder) = (0usize, 0usize);
    for f in a..b {
        let Some(&m) = mic.get(f) else { break };
        if m < VOICED_DB {
            continue;
        }
        voiced += 1;
        let s = system.get(f.saturating_sub(LAG)..(f + LAG + 1).min(system.len())).unwrap_or_default().iter().copied().fold(f32::MIN, f32::max);
        if s > m {
            louder += 1;
        }
    }
    if voiced == 0 { 0.0 } else { louder as f32 / voiced as f32 }
}

/// Your lines without the ones that are the others' heard again. `mic` and `system` are the
/// tracks' levels ([`crate::audio::levels`]).
pub fn drop_echo(you: Vec<Cue>, others: &[Cue], mic: &[f32], system: &[f32]) -> Vec<Cue> {
    let before = you.len();
    let kept: Vec<Cue> = you
        .into_iter()
        .filter(|c| {
            let len = c.end.saturating_sub(c.start).max(1);
            let overlapping: Vec<&Cue> = others.iter().filter(|o| o.start < c.end && c.start < o.end).collect();
            let overlap: u64 = overlapping.iter().map(|o| c.end.min(o.end) - c.start.max(o.start)).sum();
            if overlapping.is_empty() || overlap * 10 < len * 3 {
                return true;
            }
            let theirs: String = overlapping.iter().map(|o| o.text.as_str()).collect::<Vec<_>>().join(" ");
            !(louder_share(c.start, c.end, mic, system) >= 0.5 || shared(&c.text, &theirs) >= 0.5)
        })
        .collect();
    if kept.len() < before {
        log::info!("{} lines of yours were the others' heard again, and are left out", before - kept.len());
    }
    kept
}

/// The microphone's levels with the moments the computer's sound was clearly the louder made
/// silent: what it heard then came from the speakers, so it isn't heard as yours at all (the
/// words are the others', and hearing them twice would cost twice).
pub fn without_echo(mic: &[f32], system: &[f32]) -> Vec<f32> {
    mic.iter()
        .enumerate()
        .map(|(f, &m)| {
            let s = system.get(f.saturating_sub(LAG)..(f + LAG + 1).min(system.len())).unwrap_or_default().iter().copied().fold(f32::MIN, f32::max);
            if s > m + CLEARLY_DB { crate::audio::SILENT_DB } else { m }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use screengrabcraft_media::captions::Speaker;

    use super::*;

    fn cue(start_s: f32, end_s: f32, speaker: Speaker, text: &str) -> Cue {
        Cue { start: (start_s * 1e6) as u64, end: (end_s * 1e6) as u64, speaker, text: text.into() }
    }

    /// Levels every 20 ms for `secs`, `db` where `loud` says.
    fn levels(secs: f32, loud: &[(f32, f32, f32)]) -> Vec<f32> {
        (0..(secs * 50.0) as usize)
            .map(|i| {
                let t = i as f32 / 50.0;
                loud.iter().find(|(a, b, _)| t >= *a && t < *b).map_or(-100.0, |(_, _, db)| *db)
            })
            .collect()
    }

    #[test]
    fn the_others_heard_through_the_speakers_are_left_out_and_you_are_kept() {
        // They speak 0–4 s (loud on the computer, faint in the microphone); you speak 5–8 s.
        let system = levels(10.0, &[(0.0, 4.0, -12.0)]);
        let mic = levels(10.0, &[(0.0, 4.0, -38.0), (5.0, 8.0, -18.0)]);
        let others = vec![cue(0.2, 3.9, Speaker::Others, "The budget is due on Tuesday.")];
        let you = vec![cue(0.3, 3.9, Speaker::You, "The budget is due on Tuesday."), cue(5.1, 7.9, Speaker::You, "Thanks, I will send it.")];
        let kept = drop_echo(you, &others, &mic, &system);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].text, "Thanks, I will send it.");
    }

    #[test]
    fn what_the_microphone_heard_from_the_speakers_isnt_heard_as_yours() {
        let system = levels(6.0, &[(0.0, 3.0, -12.0), (4.0, 6.0, -15.0)]);
        // Their echo, faint and a little late; then you, over them, as loud as they are.
        let mic = levels(6.0, &[(0.06, 3.06, -36.0), (4.0, 6.0, -14.0)]);
        let heard = without_echo(&mic, &system);
        assert!(heard[10..150].iter().all(|l| *l == crate::audio::SILENT_DB));
        assert!(heard[210..290].iter().all(|l| *l == -14.0));
        // Without the computer's sound, the microphone is as it was.
        assert_eq!(without_echo(&mic, &[]), mic);
    }

    #[test]
    fn a_line_heard_wrongly_but_louder_through_the_speakers_still_goes_and_a_reply_over_them_stays() {
        let system = levels(10.0, &[(0.0, 6.0, -10.0)]);
        // The echo came out as other words; then you talk over them, louder than they are.
        let mic = levels(10.0, &[(0.0, 3.0, -35.0), (3.0, 6.0, -6.0)]);
        let others = vec![cue(0.0, 6.0, Speaker::Others, "We should ship it next month and see.")];
        let you = vec![cue(0.1, 2.9, Speaker::You, "Ship shape next mouth."), cue(3.1, 5.9, Speaker::You, "No, not before the tests pass.")];
        let kept = drop_echo(you, &others, &mic, &system);
        assert_eq!(kept.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["No, not before the tests pass."]);
        // Nothing of theirs at the time: kept, however loud the computer was (music, say).
        let lone = vec![cue(0.5, 2.5, Speaker::You, "Ship shape next mouth.")];
        assert_eq!(drop_echo(lone, &[], &mic, &system).len(), 1);
        // No levels at all (a track that couldn't be read): only the words decide.
        let you = vec![cue(0.1, 2.9, Speaker::You, "We should ship it")];
        assert!(drop_echo(you, &others, &[], &[]).is_empty());
    }
}
