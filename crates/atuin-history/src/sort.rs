use atuin_client::history::History;

type ScoredHistory = (f64, History);

// Fuzzy search already comes sorted by minspan
// This sorting should be applicable to all search modes, and solve the more "obvious" issues
// first.
// Later on, we can pass in context and do some boosts there too.
pub fn sort(query: &str, input: Vec<History>) -> Vec<History> {
    // This can totally be extended. We need to be _careful_ that it's not slow.
    // We also need to balance sorting db-side with sorting here. SQLite can do a lot,
    // but some things are just much easier/more doable in Rust.

    let mut scored = input
        .into_iter()
        .map(|h| {
            let score = base_score(query, &h);
            (score, h)
        })
        .collect::<Vec<ScoredHistory>>();

    scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap().reverse());

    // Remove the scores and return the history
    scored.into_iter().map(|(_, h)| h).collect::<Vec<History>>()
}

pub fn sort_with_match_ranking(
    query: &str,
    input: Vec<History>,
    exact_prefix_substring_sort: bool,
) -> Vec<History> {
    if !exact_prefix_substring_sort {
        return sort(query, input);
    }

    // This can totally be extended. We need to be _careful_ that it's not slow.
    // We also need to balance sorting db-side with sorting here. SQLite can do a lot,
    // but some things are just much easier/more doable in Rust.

    let mut scored = input
        .into_iter()
        .map(|h| {
            let rank = if h.command == query {
                3
            } else if h.command.starts_with(query) {
                2
            } else if h.command.contains(query) {
                1
            } else {
                0
            };

            (rank, base_score(query, &h), h)
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.partial_cmp(&a.1).unwrap()));

    // Remove the scores and return the history
    scored
        .into_iter()
        .map(|(_, _, h)| h)
        .collect::<Vec<History>>()
}

fn base_score(query: &str, h: &History) -> f64 {
    // If history is _prefixed_ with the query, score it more highly
    let score = if h.command.starts_with(query) {
        2.0
    } else if h.command.contains(query) {
        1.75
    } else {
        1.0
    };

    // calculate how long ago the history was, in seconds
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let time = h.timestamp.unix_timestamp();
    let diff = std::cmp::max(1, now - time); // no /0 please

    // prefer newer history, but not hugely so as to offset the other scoring
    // the numbers will get super small over time, but I don't want time to overpower other
    // scoring
    #[allow(clippy::cast_precision_loss)]
    let time_score = 1.0 + (1.0 / diff as f64);
    score * time_score
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Duration, OffsetDateTime};

    fn history(command: &str, timestamp: OffsetDateTime) -> History {
        History::capture()
            .timestamp(timestamp)
            .command(command)
            .cwd("/tmp")
            .build()
            .into()
    }

    #[test]
    fn ranks_exact_before_prefix_before_substring() {
        let now = OffsetDateTime::now_utc();
        let sorted = sort_with_match_ranking(
            "git",
            vec![
                history("echo git", now),
                history("git status", now - Duration::days(2)),
                history("git", now - Duration::days(4)),
            ],
            true,
        );

        let commands = sorted
            .iter()
            .map(|h| h.command.as_str())
            .collect::<Vec<_>>();
        assert_eq!(commands, vec!["git", "git status", "echo git"]);
    }

    #[test]
    fn ranks_newer_matches_within_same_match_class() {
        let now = OffsetDateTime::now_utc();
        let sorted = sort_with_match_ranking(
            "git",
            vec![
                history("git status", now - Duration::days(2)),
                history("git commit", now),
            ],
            true,
        );

        let commands = sorted
            .iter()
            .map(|h| h.command.as_str())
            .collect::<Vec<_>>();
        assert_eq!(commands, vec!["git commit", "git status"]);
    }

    #[test]
    fn ranks_fuzzy_only_matches_after_substring_matches() {
        let now = OffsetDateTime::now_utc();
        let sorted = sort_with_match_ranking(
            "git",
            vec![
                history("g i t", now),
                history("echo git", now - Duration::days(2)),
            ],
            true,
        );

        let commands = sorted
            .iter()
            .map(|h| h.command.as_str())
            .collect::<Vec<_>>();
        assert_eq!(commands, vec!["echo git", "g i t"]);
    }
}
