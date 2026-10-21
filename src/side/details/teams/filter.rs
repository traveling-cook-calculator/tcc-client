//! Search and sort of the team list. Pure functions, no UI.
//!
//! Works on `TeamMetaData` (one cheap request for the whole list). Contact data is not part
//! of it, so the search covers the team name.

use crate::storage::{TeamMetaData, TeamStatus};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TeamSort {
    /// A–Z.
    #[default]
    Name,
    /// Teams that need attention first: review, active, canceled.
    Status,
    /// Most recently registered first.
    Newest,
}

impl TeamSort {
    pub const ALL: [(TeamSort, &'static str); 3] =
        [(TeamSort::Name, "Name"), (TeamSort::Status, "Status"), (TeamSort::Newest, "Newest")];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|(sort, _)| *sort == self).unwrap_or(0)
    }
}

fn status_rank(status: TeamStatus) -> u8 {
    match status {
        TeamStatus::Review => 0,
        TeamStatus::Active => 1,
        TeamStatus::Canceled => 2,
    }
}

/// The teams to show: filtered by `query` (blank = all) and sorted.
pub fn visible_teams<'a>(
    teams: &'a [TeamMetaData],
    query: &str,
    sort: TeamSort,
) -> Vec<&'a TeamMetaData> {
    let query = query.trim().to_lowercase();
    let mut shown: Vec<&TeamMetaData> = teams
        .iter()
        .filter(|team| query.is_empty() || team.name.to_lowercase().contains(&query))
        .collect();

    // Ties always fall back to the name so the order is stable and predictable.
    shown.sort_by(|a, b| {
        let by_name = a.name.to_lowercase().cmp(&b.name.to_lowercase());
        match sort {
            TeamSort::Name => by_name,
            TeamSort::Status => status_rank(a.status).cmp(&status_rank(b.status)).then(by_name),
            TeamSort::Newest => b.created.cmp(&a.created).then(by_name),
        }
    });
    shown
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use uuid::Uuid;

    use super::*;

    fn team(name: &str, status: TeamStatus, day: u32) -> TeamMetaData {
        let created = NaiveDate::from_ymd_opt(2026, 5, day).unwrap().and_hms_opt(12, 0, 0).unwrap();
        TeamMetaData {
            id: Uuid::new_v4(),
            name: name.to_string(),
            mail: false,
            phone: false,
            members: None,
            diets: false,
            created_by_user: None,
            created,
            edited: created,
            status,
            canceled_at: None,
            email_verified: false,
        }
    }

    fn names(teams: &[&TeamMetaData]) -> Vec<String> {
        teams.iter().map(|team| team.name.clone()).collect()
    }

    fn sample() -> Vec<TeamMetaData> {
        vec![
            team("charlie", TeamStatus::Active, 3),
            team("Alpha", TeamStatus::Canceled, 1),
            team("Bravo", TeamStatus::Review, 2),
            team("delta", TeamStatus::Review, 4),
        ]
    }

    #[test]
    fn sorts_by_name_ignoring_case() {
        let teams = sample();
        assert_eq!(names(&visible_teams(&teams, "", TeamSort::Name)), ["Alpha", "Bravo", "charlie", "delta"]);
    }

    #[test]
    fn status_puts_review_first_then_active_then_canceled() {
        let teams = sample();
        assert_eq!(names(&visible_teams(&teams, "", TeamSort::Status)), ["Bravo", "delta", "charlie", "Alpha"]);
    }

    #[test]
    fn newest_comes_first() {
        let teams = sample();
        assert_eq!(names(&visible_teams(&teams, "", TeamSort::Newest)), ["delta", "charlie", "Bravo", "Alpha"]);
    }

    #[test]
    fn search_matches_name_case_insensitively_and_trims() {
        let teams = sample();
        assert_eq!(names(&visible_teams(&teams, "  ALP ", TeamSort::Name)), ["Alpha"]);
        assert_eq!(names(&visible_teams(&teams, "a", TeamSort::Name)).len(), 4);
        assert!(visible_teams(&teams, "zzz", TeamSort::Name).is_empty());
    }

    #[test]
    fn blank_query_shows_everything() {
        let teams = sample();
        assert_eq!(visible_teams(&teams, "   ", TeamSort::Name).len(), 4);
    }

    #[test]
    fn filter_and_sort_work_together() {
        let teams = sample();
        let shown = visible_teams(&teams, "l", TeamSort::Status);
        assert_eq!(names(&shown), ["delta", "charlie", "Alpha"]);
    }
}
