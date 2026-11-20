//! The rules. [`evaluate`] builds the full report for the UI, [`Scorer`] computes only the
//! score and is fast enough to try thousands of changes (both always agree, see the tests).

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use super::model::{Context, Draft};
use crate::storage::{MAX_GUESTS_PER_HOSTING, MIN_GUESTS_PER_HOSTING};

/// One repeated meeting is as bad as this many "points" of uneven distribution.
pub const REPEAT_WEIGHT: u32 = 100;
/// `sum((persons - mean)^2)` over all tables of all courses is multiplied by this.
pub const IMBALANCE_SCALE: f64 = 10.0;

/// The hard rules, as groups for the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rule {
    /// Every team cooks exactly once.
    CookOnce,
    /// Every team has exactly one seat in every course.
    OneSeatPerCourse,
    /// Every table has a host and 1..=5 guests.
    TableSize,
}

impl Rule {
    pub const ALL: [Rule; 3] = [Rule::CookOnce, Rule::OneSeatPerCourse, Rule::TableSize];

    pub fn title(self) -> &'static str {
        match self {
            Rule::CookOnce => "Every team cooks exactly once",
            Rule::OneSeatPerCourse => "Every team has exactly one place in every course",
            Rule::TableSize => "Every table has a host and 1–5 guests",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Violation {
    NeverCooks {
        team: Uuid,
    },
    CooksMultiple {
        team: Uuid,
        courses: Vec<Uuid>,
    },
    MissingInCourse {
        team: Uuid,
        course: Uuid,
    },
    DoubleBooked {
        team: Uuid,
        course: Uuid,
    },
    HostMissing {
        table: usize,
        course: Uuid,
    },
    TooFewGuests {
        table: usize,
        course: Uuid,
        count: usize,
    },
    TooManyGuests {
        table: usize,
        course: Uuid,
        count: usize,
    },
}

impl Violation {
    pub fn rule(&self) -> Rule {
        match self {
            Violation::NeverCooks { .. } | Violation::CooksMultiple { .. } => Rule::CookOnce,
            Violation::MissingInCourse { .. } | Violation::DoubleBooked { .. } => {
                Rule::OneSeatPerCourse
            }
            Violation::HostMissing { .. }
            | Violation::TooFewGuests { .. }
            | Violation::TooManyGuests { .. } => Rule::TableSize,
        }
    }

    pub fn team(&self) -> Option<Uuid> {
        match self {
            Violation::NeverCooks { team }
            | Violation::CooksMultiple { team, .. }
            | Violation::MissingInCourse { team, .. }
            | Violation::DoubleBooked { team, .. } => Some(*team),
            _ => None,
        }
    }

    pub fn table(&self) -> Option<usize> {
        match self {
            Violation::HostMissing { table, .. }
            | Violation::TooFewGuests { table, .. }
            | Violation::TooManyGuests { table, .. } => Some(*table),
            _ => None,
        }
    }
}

/// Smaller is better. The hard rules always come first: a plan with one hard violation is worse
/// than any plan without.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Score {
    pub hard: u32,
    pub soft: u32,
}

/// Two teams that sit at the same table.
#[derive(Clone, Debug, PartialEq)]
pub struct Meeting {
    pub a: Uuid,
    pub b: Uuid,
    pub count: u32,
    pub courses: Vec<Uuid>,
}

impl Meeting {
    pub fn involves(&self, team: Uuid) -> bool {
        self.a == team || self.b == team
    }

    pub fn other(&self, team: Uuid) -> Uuid {
        if self.a == team { self.b } else { self.a }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableStat {
    pub table: usize,
    pub host: Option<Uuid>,
    pub persons: u32,
    pub guests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CourseStat {
    pub course: Uuid,
    pub tables: Vec<TableStat>,
    pub min_persons: u32,
    pub max_persons: u32,
    pub mean_persons: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub violations: Vec<Violation>,
    pub courses: Vec<CourseStat>,
    /// Every pair of teams that sits together at least once.
    pub meetings: Vec<Meeting>,
    /// `sum((persons - mean)^2)` over all tables.
    pub imbalance: f64,
    pub score: Score,
}

impl Report {
    pub fn hard_ok(&self) -> bool {
        self.violations.is_empty()
    }

    /// Pairs that meet more than once.
    pub fn repeats(&self) -> Vec<&Meeting> {
        self.meetings
            .iter()
            .filter(|meeting| meeting.count > 1)
            .collect()
    }

    pub fn repeat_count(&self) -> u32 {
        self.meetings
            .iter()
            .map(|meeting| meeting.count.saturating_sub(1))
            .sum()
    }

    /// `true` if all courses have the same number of persons at every table.
    pub fn balanced(&self) -> bool {
        self.imbalance < 1e-9
    }

    pub fn violations_of(&self, rule: Rule) -> Vec<&Violation> {
        self.violations
            .iter()
            .filter(|violation| violation.rule() == rule)
            .collect()
    }

    /// Everybody `team` meets, with the number of shared tables.
    pub fn met_by(&self, team: Uuid) -> Vec<(Uuid, u32)> {
        self.meetings
            .iter()
            .filter(|meeting| meeting.involves(team))
            .map(|meeting| (meeting.other(team), meeting.count))
            .collect()
    }

    pub fn meeting_count(&self, a: Uuid, b: Uuid) -> u32 {
        self.meetings
            .iter()
            .find(|meeting| {
                (meeting.a == a && meeting.b == b) || (meeting.a == b && meeting.b == a)
            })
            .map_or(0, |meeting| meeting.count)
    }

    /// Teams that are part of a violation of a hard rule or of a repeated meeting.
    pub fn problem_teams(&self) -> HashSet<Uuid> {
        let mut teams = HashSet::new();
        for violation in &self.violations {
            teams.extend(violation.team());
        }
        for meeting in self.repeats() {
            teams.insert(meeting.a);
            teams.insert(meeting.b);
        }
        teams
    }
}

fn is_empty_table(table: &super::model::Table) -> bool {
    table.host.is_none() && table.guests.is_empty()
}

/// Checks `draft` against all rules.
pub fn evaluate(draft: &Draft, ctx: &Context) -> Report {
    let mut violations = Vec::new();
    let known: HashSet<Uuid> = ctx.teams.iter().map(|team| team.id).collect();

    // Rule: every table has a host and 1..=5 guests.
    for (index, table) in draft.tables.iter().enumerate() {
        if is_empty_table(table) || ctx.course(table.course).is_none() {
            continue;
        }
        let count = table.guests.len();
        if table.host.is_none() {
            violations.push(Violation::HostMissing {
                table: index,
                course: table.course,
            });
        }
        if count < MIN_GUESTS_PER_HOSTING {
            violations.push(Violation::TooFewGuests {
                table: index,
                course: table.course,
                count,
            });
        }
        if count > MAX_GUESTS_PER_HOSTING {
            violations.push(Violation::TooManyGuests {
                table: index,
                course: table.course,
                count,
            });
        }
    }

    // Rule: every team cooks exactly once / has exactly one seat per course.
    for team in &ctx.teams {
        let cooks: Vec<Uuid> = draft
            .tables
            .iter()
            .filter(|table| table.host == Some(team.id))
            .map(|table| table.course)
            .collect();
        match cooks.len() {
            0 => violations.push(Violation::NeverCooks { team: team.id }),
            1 => {}
            _ => violations.push(Violation::CooksMultiple {
                team: team.id,
                courses: cooks,
            }),
        }
        for course in &ctx.courses {
            let seats: usize = draft
                .tables_of(course.id)
                .map(|(_, table)| {
                    (table.host == Some(team.id)) as usize
                        + table
                            .guests
                            .iter()
                            .filter(|guest| **guest == team.id)
                            .count()
                })
                .sum();
            match seats {
                0 => violations.push(Violation::MissingInCourse {
                    team: team.id,
                    course: course.id,
                }),
                1 => {}
                _ => violations.push(Violation::DoubleBooked {
                    team: team.id,
                    course: course.id,
                }),
            }
        }
    }

    // Soft rules.
    let mut pair_courses: HashMap<(Uuid, Uuid), Vec<Uuid>> = HashMap::new();
    let mut courses = Vec::new();
    let mut imbalance = 0.0;
    for course in &ctx.courses {
        let mut stats = Vec::new();
        for (index, table) in draft.tables_of(course.id) {
            if is_empty_table(table) {
                continue;
            }
            let members: Vec<Uuid> = table.members().filter(|id| known.contains(id)).collect();
            let persons: u32 = members.iter().map(|id| ctx.persons(*id)).sum();
            for (i, a) in members.iter().enumerate() {
                for b in &members[i + 1..] {
                    let key = if a < b { (*a, *b) } else { (*b, *a) };
                    pair_courses.entry(key).or_default().push(course.id);
                }
            }
            stats.push(TableStat {
                table: index,
                host: table.host,
                persons,
                guests: table.guests.len(),
            });
        }
        let count = stats.len() as f64;
        let sum: f64 = stats.iter().map(|stat| stat.persons as f64).sum();
        let sum_sq: f64 = stats.iter().map(|stat| (stat.persons as f64).powi(2)).sum();
        if count > 0.0 {
            imbalance += (sum_sq - sum * sum / count).max(0.0);
        }
        courses.push(CourseStat {
            course: course.id,
            min_persons: stats.iter().map(|stat| stat.persons).min().unwrap_or(0),
            max_persons: stats.iter().map(|stat| stat.persons).max().unwrap_or(0),
            mean_persons: if count > 0.0 { sum / count } else { 0.0 },
            tables: stats,
        });
    }

    let mut meetings: Vec<Meeting> = pair_courses
        .into_iter()
        .map(|((a, b), courses)| Meeting {
            a,
            b,
            count: courses.len() as u32,
            courses,
        })
        .collect();
    meetings.sort_by(|x, y| {
        y.count
            .cmp(&x.count)
            .then(x.a.cmp(&y.a))
            .then(x.b.cmp(&y.b))
    });

    let repeats: u32 = meetings
        .iter()
        .map(|meeting| meeting.count.saturating_sub(1))
        .sum();
    let score = Score {
        hard: violations.len() as u32,
        soft: repeats * REPEAT_WEIGHT + (imbalance * IMBALANCE_SCALE).round() as u32,
    };
    Report {
        violations,
        courses,
        meetings,
        imbalance,
        score,
    }
}

/// The same score as [`evaluate`], without building the report. Create it once per context.
pub struct Scorer {
    index: HashMap<Uuid, usize>,
    course_pos: HashMap<Uuid, usize>,
    persons: Vec<u32>,
}

impl Scorer {
    pub fn new(ctx: &Context) -> Self {
        Self {
            index: Draft::index_of(ctx),
            course_pos: ctx
                .courses
                .iter()
                .enumerate()
                .map(|(i, course)| (course.id, i))
                .collect(),
            persons: ctx
                .teams
                .iter()
                .map(|team| team.persons.max(1) as u32)
                .collect(),
        }
    }

    pub fn score(&self, draft: &Draft) -> Score {
        let n = self.persons.len();
        let c = self.course_pos.len();
        let mut cooks = vec![0u8; n];
        let mut seats = vec![0u8; n * c];
        let mut pairs = vec![0u8; n * n];
        let (mut hard, mut repeats) = (0u32, 0u32);
        let (mut sum, mut sum_sq, mut count) = (vec![0f64; c], vec![0f64; c], vec![0f64; c]);
        let mut members: Vec<usize> = Vec::with_capacity(8);

        for table in &draft.tables {
            let Some(&course) = self.course_pos.get(&table.course) else {
                continue;
            };
            if is_empty_table(table) {
                continue;
            }
            let guests = table.guests.len();
            hard += table.host.is_none() as u32
                + (guests < MIN_GUESTS_PER_HOSTING) as u32
                + (guests > MAX_GUESTS_PER_HOSTING) as u32;

            members.clear();
            let mut persons = 0u32;
            for id in table.members() {
                if let Some(&team) = self.index.get(&id) {
                    members.push(team);
                    persons += self.persons[team];
                    let seat = &mut seats[team * c + course];
                    *seat = seat.saturating_add(1);
                }
            }
            if let Some(&host) = table.host.as_ref().and_then(|id| self.index.get(id)) {
                cooks[host] = cooks[host].saturating_add(1);
            }
            for (i, &a) in members.iter().enumerate() {
                for &b in &members[i + 1..] {
                    let (low, high) = if a < b { (a, b) } else { (b, a) };
                    let cell = &mut pairs[low * n + high];
                    *cell = cell.saturating_add(1);
                    if *cell >= 2 {
                        repeats += 1;
                    }
                }
            }
            let persons = persons as f64;
            sum[course] += persons;
            sum_sq[course] += persons * persons;
            count[course] += 1.0;
        }

        for team in 0..n {
            hard += (cooks[team] != 1) as u32;
            for course in 0..c {
                hard += (seats[team * c + course] != 1) as u32;
            }
        }
        let imbalance: f64 = (0..c)
            .filter(|course| count[*course] > 0.0)
            .map(|course| (sum_sq[course] - sum[course] * sum[course] / count[course]).max(0.0))
            .sum();
        Score {
            hard,
            soft: repeats * REPEAT_WEIGHT + (imbalance * IMBALANCE_SCALE).round() as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::Op;
    use super::super::model::test_support::*;
    use super::*;

    #[test]
    fn the_calculator_style_plan_is_perfect() {
        let (ctx, draft) = nine();
        let report = evaluate(&draft, &ctx);
        assert!(report.hard_ok(), "{:?}", report.violations);
        assert_eq!(report.repeat_count(), 0);
        assert!(report.balanced());
        assert_eq!(report.score, Score::default());
        // 3 courses x 3 tables x 3 pairs
        assert_eq!(report.meetings.len(), 27);
    }

    #[test]
    fn missing_cook_and_missing_seat_are_hard_violations() {
        let (ctx, mut draft) = nine();
        // Team 3 is taken out of course 1; its table (index 0) keeps host 1 and guest 2.
        draft.tables[0].guests.retain(|guest| *guest != id(3));
        let report = evaluate(&draft, &ctx);
        assert_eq!(
            report.violations,
            vec![Violation::MissingInCourse {
                team: id(3),
                course: id(101)
            }]
        );

        // Team 1 hosts twice and team 4 never.
        let (ctx, mut draft) = nine();
        draft.tables[1].host = Some(id(1));
        draft.tables[1].guests.retain(|guest| *guest != id(1));
        let report = evaluate(&draft, &ctx);
        assert!(
            report
                .violations
                .iter()
                .any(|v| matches!(v, Violation::NeverCooks { team } if *team == id(4)))
        );
        assert!(
            report
                .violations
                .iter()
                .any(|v| matches!(v, Violation::CooksMultiple { team, .. } if *team == id(1)))
        );
        assert!(
            report
                .violations
                .iter()
                .any(|v| matches!(v, Violation::DoubleBooked { team, .. } if *team == id(1)))
        );
        assert_eq!(report.violations_of(Rule::CookOnce).len(), 2);
    }

    #[test]
    fn table_size_limits() {
        let (ctx, mut draft) = nine();
        draft.tables[0].guests.clear();
        draft.tables[1].host = None;
        let report = evaluate(&draft, &ctx);
        assert!(
            report
                .violations
                .iter()
                .any(|v| matches!(v, Violation::TooFewGuests { table: 0, .. }))
        );
        assert!(
            report
                .violations
                .iter()
                .any(|v| matches!(v, Violation::HostMissing { table: 1, .. }))
        );
    }

    #[test]
    fn repeated_meetings_are_counted_per_pair() {
        let (ctx, mut draft) = nine();
        // Swap team 5 (course 2, table of team 2) with team 3 -> 3 meets 2 again? Build a repeat
        // directly: put team 2 also at team 1's table in course 2.
        draft = draft
            .applied(&Op::Swap {
                course: id(102),
                a: id(1),
                b: id(8),
            })
            .unwrap();
        let report = evaluate(&draft, &ctx);
        assert!(report.repeat_count() > 0);
        assert!(report.score.soft >= REPEAT_WEIGHT);
        assert_eq!(report.score.hard, 0);
        let repeat = report.repeats()[0];
        assert!(repeat.count >= 2 && repeat.courses.len() >= 2);
    }

    #[test]
    fn imbalance_uses_the_number_of_persons() {
        let (mut ctx, draft) = nine();
        assert!(evaluate(&draft, &ctx).balanced());
        ctx.teams[0].persons = 4; // team 1 is bigger: its tables are fuller
        let report = evaluate(&draft, &ctx);
        assert!(!report.balanced());
        assert_eq!(report.courses[0].max_persons, 8);
        assert_eq!(report.courses[0].min_persons, 6);
    }

    #[test]
    fn fast_score_agrees_with_the_report() {
        let (mut ctx, draft) = nine();
        ctx.teams[3].persons = 3;
        let scorer = Scorer::new(&ctx);
        let courses = [id(101), id(102), id(103)];
        let teams: Vec<Uuid> = ctx.teams.iter().map(|team| team.id).collect();
        let mut current = draft;
        // A deterministic pseudo random walk through lots of (also broken) plans.
        let mut seed = 12345u64;
        let mut next = move |limit: usize| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % limit
        };
        for _ in 0..400 {
            let course = courses[next(3)];
            let a = teams[next(teams.len())];
            let b = teams[next(teams.len())];
            let op = match next(4) {
                0 => Op::Swap { course, a, b },
                1 => Op::Unseat { course, team: a },
                2 => Op::SwapRoles { a, b },
                _ => Op::Move {
                    course,
                    team: a,
                    to_table: next(current.tables.len()),
                },
            };
            if let Ok(changed) = current.applied(&op) {
                current = changed;
            }
            assert_eq!(
                scorer.score(&current),
                evaluate(&current, &ctx).score,
                "after {op:?}"
            );
        }
    }
}
