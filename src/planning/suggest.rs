//! Proposals: which single change improves the plan most, automatic repair, and who is affected
//! by changes of the teams.

use std::collections::HashSet;

use uuid::Uuid;

use super::model::{Context, Draft, Op, Sanitized};
use super::rules::{Report, Score, Scorer, evaluate};
use crate::storage::MAX_GUESTS_PER_HOSTING;

/// Above this many teams only teams that are part of a problem are tried (keeps the search fast).
const FULL_SEARCH_LIMIT: usize = 40;
/// Never try more changes than this in one search.
const MAX_CANDIDATES: usize = 6_000;

/// What a change does to the rules, before → after.
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    pub hard: (u32, u32),
    pub repeats: (u32, u32),
    pub imbalance: (f64, f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub op: Op,
    pub before: Score,
    pub after: Score,
    pub effect: Effect,
}

fn effect(before: &Report, after: &Report) -> Effect {
    Effect {
        hard: (before.score.hard, after.score.hard),
        repeats: (before.repeat_count(), after.repeat_count()),
        imbalance: (before.imbalance, after.imbalance),
    }
}

/// How many "steps" an `Op` is for the user. Simple changes are preferred at equal score.
fn effort(op: &Op) -> u8 {
    match op {
        Op::Move { .. } | Op::Swap { .. } | Op::SeatHost { .. } | Op::Unseat { .. } => 0,
        Op::SwapRoles { .. } | Op::OpenTable { .. } => 1,
    }
}

fn involved_teams(draft: &Draft, report: &Report) -> HashSet<Uuid> {
    let mut teams = report.problem_teams();
    for violation in &report.violations {
        if let Some(table) = violation.table() {
            teams.extend(draft.tables[table].members());
        }
    }
    teams
}

fn candidates(
    draft: &Draft,
    ctx: &Context,
    focus: Option<Uuid>,
    involved: Option<&HashSet<Uuid>>,
) -> Vec<Op> {
    let wanted = |a: Uuid, b: Uuid| -> bool {
        match (focus, involved) {
            (Some(f), _) => a == f || b == f,
            (None, Some(set)) => set.contains(&a) || set.contains(&b),
            (None, None) => true,
        }
    };
    let mut ops: Vec<Op> = Vec::new();

    for course in &ctx.courses {
        let tables: Vec<(usize, &super::model::Table)> = draft
            .tables_of(course.id)
            .filter(|(_, table)| table.host.is_some() || !table.guests.is_empty())
            .collect();
        let unseated = draft.unseated(course.id, ctx);

        // Moves of guests and of teams without seat to another table.
        let movers = tables
            .iter()
            .flat_map(|(index, table)| table.guests.iter().map(move |guest| (Some(*index), *guest)))
            .chain(unseated.iter().map(|team| (None, *team)));
        for (from, team) in movers {
            if !wanted(team, team) {
                continue;
            }
            for (to, table) in &tables {
                if Some(*to) != from && table.guests.len() < MAX_GUESTS_PER_HOSTING {
                    ops.push(Op::Move {
                        course: course.id,
                        team,
                        to_table: *to,
                    });
                }
            }
        }

        // Swaps of two seated teams at different tables.
        let seated: Vec<(usize, Uuid)> = tables
            .iter()
            .flat_map(|(index, table)| table.members().map(move |team| (*index, team)))
            .collect();
        for (i, (table_a, a)) in seated.iter().enumerate() {
            for (table_b, b) in &seated[i + 1..] {
                if table_a != table_b && wanted(*a, *b) {
                    ops.push(Op::Swap {
                        course: course.id,
                        a: *a,
                        b: *b,
                    });
                }
            }
        }

        // Somebody takes over a table without host.
        for (index, table) in &tables {
            if table.host.is_none() {
                for team in ctx
                    .teams
                    .iter()
                    .map(|team| team.id)
                    .filter(|team| wanted(*team, *team))
                {
                    ops.push(Op::SeatHost {
                        table: *index,
                        team,
                    });
                }
            }
        }

        // A team that does not cook opens its own table and takes guests from the fullest ones.
        let mut crowded: Vec<&(usize, &super::model::Table)> = tables
            .iter()
            .filter(|(_, table)| table.guests.len() >= 2)
            .collect();
        crowded.sort_by_key(|(_, table)| std::cmp::Reverse(table.guests.len()));
        crowded.truncate(10);
        for team in ctx.teams.iter().map(|team| team.id) {
            if draft.cooking_course(team).is_some() || !wanted(team, team) {
                continue;
            }
            for (i, (_, first)) in crowded.iter().enumerate() {
                let Some(a) = first.guests.iter().rev().find(|guest| **guest != team) else {
                    continue;
                };
                if let Some(b) = first
                    .guests
                    .iter()
                    .rev()
                    .find(|guest| **guest != team && *guest != a)
                {
                    ops.push(Op::OpenTable {
                        course: course.id,
                        host: team,
                        take: vec![*a, *b],
                    });
                }
                for (_, second) in &crowded[i + 1..] {
                    if let Some(b) = second.guests.iter().rev().find(|guest| **guest != team) {
                        ops.push(Op::OpenTable {
                            course: course.id,
                            host: team,
                            take: vec![*a, *b],
                        });
                    }
                }
            }
        }
    }

    // Two teams exchange who cooks which course.
    for (i, a) in ctx.teams.iter().enumerate() {
        for b in &ctx.teams[i + 1..] {
            let cooks =
                draft.cooking_course(a.id).is_some() || draft.cooking_course(b.id).is_some();
            if cooks && wanted(a.id, b.id) {
                ops.push(Op::SwapRoles { a: a.id, b: b.id });
            }
        }
    }

    ops.truncate(MAX_CANDIDATES);
    ops
}

/// All changes that make the plan better, best first. With `focus` only changes that involve this
/// team are tried.
pub fn suggest(draft: &Draft, ctx: &Context, focus: Option<Uuid>, limit: usize) -> Vec<Suggestion> {
    let scorer = Scorer::new(ctx);
    let before_report = evaluate(draft, ctx);
    let before = before_report.score;
    let involved = (focus.is_none() && ctx.teams.len() > FULL_SEARCH_LIMIT)
        .then(|| involved_teams(draft, &before_report));

    let mut better: Vec<(Op, Score)> = Vec::new();
    for op in candidates(draft, ctx, focus, involved.as_ref()) {
        let Ok(next) = draft.applied(&op) else {
            continue;
        };
        let score = scorer.score(&next);
        if score < before {
            better.push((op, score));
        }
    }
    better.sort_by_key(|(op, score)| (*score, effort(op)));
    better.truncate(limit);

    better
        .into_iter()
        .filter_map(|(op, after)| {
            let next = draft.applied(&op).ok()?;
            Some(Suggestion {
                effect: effect(&before_report, &evaluate(&next, ctx)),
                op,
                before,
                after,
            })
        })
        .collect()
}

/// Result of [`improve`].
#[derive(Clone, Debug, PartialEq)]
pub struct Improvement {
    pub draft: Draft,
    /// How many seats (team in a course) are different from before.
    pub changes: usize,
    pub before: Score,
    pub after: Score,
}

/// Applies the best change again and again (at most `max_steps` times) until no single change
/// helps any more. Fast and deterministic, but it stops at the first dead end.
fn greedy(draft: &Draft, ctx: &Context, scorer: &Scorer, max_steps: usize) -> Draft {
    let mut current = draft.clone();
    let mut score = scorer.score(&current);

    for _ in 0..max_steps {
        if score == Score::default() {
            break;
        }
        let involved = (ctx.teams.len() > FULL_SEARCH_LIMIT)
            .then(|| involved_teams(&current, &evaluate(&current, ctx)));
        let mut best: Option<(Op, Draft, Score)> = None;
        for op in candidates(&current, ctx, None, involved.as_ref()) {
            let Ok(next) = current.applied(&op) else {
                continue;
            };
            let next_score = scorer.score(&next);
            let beats = |other: &Score, other_op: &Op| {
                (next_score, effort(&op)) < (*other, effort(other_op))
            };
            if next_score < score && best.as_ref().map_or(true, |(o, _, s)| beats(s, o)) {
                best = Some((op, next, next_score));
            }
        }
        let Some((_, next, next_score)) = best else {
            break;
        };
        current = next;
        score = next_score;
    }
    current
}

/// Small deterministic random numbers (the same plan always gives the same result).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, limit: usize) -> usize {
        (self.next() % limit.max(1) as u64) as usize
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// One random change: swap two teams, move a guest, or swap two cooking roles.
fn random_op(draft: &Draft, ctx: &Context, rng: &mut Rng) -> Option<Op> {
    let course = ctx.courses.get(rng.below(ctx.courses.len()))?.id;
    let teams = &ctx.teams;
    match rng.below(10) {
        0..=5 => {
            let seated: Vec<Uuid> = draft
                .tables_of(course)
                .flat_map(|(_, table)| table.members().collect::<Vec<_>>())
                .collect();
            if seated.len() < 2 {
                return None;
            }
            let a = seated[rng.below(seated.len())];
            let b = seated[rng.below(seated.len())];
            (a != b).then_some(Op::Swap { course, a, b })
        }
        6..=7 => {
            let team = teams.get(rng.below(teams.len()))?.id;
            let tables: Vec<usize> = draft
                .tables_of(course)
                .filter(|(_, table)| table.guests.len() < MAX_GUESTS_PER_HOSTING)
                .map(|(index, _)| index)
                .collect();
            let to_table = *tables.get(rng.below(tables.len()))?;
            Some(Op::Move {
                course,
                team,
                to_table,
            })
        }
        8 => {
            let a = teams.get(rng.below(teams.len()))?.id;
            let b = teams.get(rng.below(teams.len()))?.id;
            (a != b).then_some(Op::SwapRoles { a, b })
        }
        _ => {
            let unseated = draft.unseated(course, ctx);
            let team = *unseated.get(rng.below(unseated.len()))?;
            let tables: Vec<usize> = draft.tables_of(course).map(|(index, _)| index).collect();
            let table = *tables.get(rng.below(tables.len()))?;
            if draft.tables[table].host.is_none() {
                Some(Op::SeatHost { table, team })
            } else {
                Some(Op::Move {
                    course,
                    team,
                    to_table: table,
                })
            }
        }
    }
}

/// Simulated annealing: also accepts changes that make the plan a little worse for a while, which
/// is how it gets out of the dead ends `greedy` stops in. Returns the best plan it saw.
fn anneal(draft: &Draft, ctx: &Context, scorer: &Scorer, iterations: usize) -> Draft {
    let value = |score: Score| score.hard as f64 * 100_000.0 + score.soft as f64;
    let mut rng =
        Rng(0x9E37_79B9_7F4A_7C15 ^ (draft.tables.len() as u64) << 32 ^ ctx.teams.len() as u64);
    let mut current = draft.clone();
    let mut current_value = value(scorer.score(&current));
    let mut best = current.clone();
    let mut best_value = current_value;
    let (start_temperature, end_temperature) = (60.0f64, 1.0f64);

    for step in 0..iterations {
        if best_value == 0.0 {
            break;
        }
        let Some(op) = random_op(&current, ctx, &mut rng) else {
            continue;
        };
        let Ok(next) = current.applied(&op) else {
            continue;
        };
        let next_value = value(scorer.score(&next));
        let progress = step as f64 / iterations as f64;
        let temperature = start_temperature * (end_temperature / start_temperature).powf(progress);
        let delta = next_value - current_value;
        if delta <= 0.0 || rng.unit() < (-delta / temperature).exp() {
            current = next;
            current_value = next_value;
            if current_value < best_value {
                best = current.clone();
                best_value = current_value;
            }
        }
    }
    best
}

/// Makes the plan as good as possible: first the best single changes, then a search that also
/// leaves dead ends, then the single changes again. Never returns a worse plan than the given one.
pub fn improve(draft: &Draft, ctx: &Context) -> Improvement {
    let scorer = Scorer::new(ctx);
    let before = scorer.score(draft);

    let mut best = greedy(draft, ctx, &scorer, 60);
    if scorer.score(&best) != Score::default() {
        let iterations = (60_000 / ctx.teams.len().max(1)).clamp(2_000, 8_000);
        let searched = anneal(&best, ctx, &scorer, iterations);
        let polished = greedy(&searched, ctx, &scorer, 30);
        if scorer.score(&polished) < scorer.score(&best) {
            best = polished;
        }
    }
    let after = scorer.score(&best);
    let (best, after) = if after <= before {
        (best, after)
    } else {
        (draft.clone(), before)
    };

    let changes = ctx
        .courses
        .iter()
        .map(|course| {
            ctx.teams
                .iter()
                .filter(|team| {
                    let seat_of = |plan: &Draft| {
                        plan.seat(course.id, team.id).map(|seat| {
                            let table = &plan.tables[seat.table()];
                            (table.host, matches!(seat, super::model::Seat::Host(_)), {
                                let mut guests = table.guests.clone();
                                guests.sort();
                                guests
                            })
                        })
                    };
                    seat_of(draft) != seat_of(&best)
                })
                .count()
        })
        .sum();
    Improvement {
        draft: best,
        changes,
        before,
        after,
    }
}

// ─────────────────────────────────────────────
//  Who is affected?
// ─────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum Reason {
    /// The team has no place in this course.
    NoSeat { course: Uuid },
    /// The host of this team's table in this course left.
    HostLeft { course: Uuid },
    /// A guest at the table of this team in this course left.
    TableChanged { course: Uuid },
    /// The team does not cook.
    NoCookingRole,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Affected {
    pub team: Uuid,
    pub reasons: Vec<Reason>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Diagnosis {
    pub affected: Vec<Affected>,
    /// Teams that are in the stored plan but do not take part (any more).
    pub ghosts: Vec<Uuid>,
}

impl Diagnosis {
    pub fn is_clean(&self) -> bool {
        self.affected.is_empty() && self.ghosts.is_empty()
    }

    pub fn reasons_of(&self, team: Uuid) -> &[Reason] {
        self.affected
            .iter()
            .find(|a| a.team == team)
            .map_or(&[], |a| a.reasons.as_slice())
    }
}

/// Which teams need attention because the plan no longer fits the teams.
pub fn diagnose(draft: &Draft, ctx: &Context, sanitized: &Sanitized) -> Diagnosis {
    let mut reasons: Vec<(Uuid, Reason)> = Vec::new();

    for team in &ctx.teams {
        for course in &ctx.courses {
            if draft.seat(course.id, team.id).is_none() {
                reasons.push((team.id, Reason::NoSeat { course: course.id }));
            }
        }
        if draft.cooking_course(team.id).is_none() {
            reasons.push((team.id, Reason::NoCookingRole));
        }
    }
    for (index, table) in draft.tables.iter().enumerate() {
        if table.host.is_none() {
            for guest in &table.guests {
                reasons.push((
                    *guest,
                    Reason::HostLeft {
                        course: table.course,
                    },
                ));
            }
        } else if sanitized.changed_tables.contains(&index) {
            for member in table.members() {
                reasons.push((
                    member,
                    Reason::TableChanged {
                        course: table.course,
                    },
                ));
            }
        }
    }

    let mut affected: Vec<Affected> = Vec::new();
    for team in &ctx.teams {
        let mut mine: Vec<Reason> = reasons
            .iter()
            .filter(|(id, _)| *id == team.id)
            .map(|(_, reason)| reason.clone())
            .collect();
        mine.dedup();
        if !mine.is_empty() {
            affected.push(Affected {
                team: team.id,
                reasons: mine,
            });
        }
    }
    Diagnosis {
        affected,
        ghosts: sanitized.ghosts(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::test_support::*;
    use super::super::model::{CourseInfo, TeamInfo};
    use super::*;

    #[test]
    fn nothing_to_suggest_for_a_perfect_plan() {
        let (ctx, draft) = nine();
        assert!(suggest(&draft, &ctx, None, 5).is_empty());
        let result = improve(&draft, &ctx);
        assert_eq!(result.changes, 0);
        assert_eq!(result.draft, draft);
    }

    #[test]
    fn a_repeated_meeting_gets_a_fix() {
        let (ctx, draft) = nine();
        let broken = draft
            .applied(&Op::Swap {
                course: id(102),
                a: id(1),
                b: id(8),
            })
            .unwrap();
        let before = evaluate(&broken, &ctx);
        assert!(before.repeat_count() > 0);

        let suggestions = suggest(&broken, &ctx, None, 3);
        assert!(!suggestions.is_empty());
        assert!(suggestions[0].after < suggestions[0].before);
        assert!(suggestions[0].effect.repeats.1 < suggestions[0].effect.repeats.0);

        let result = improve(&broken, &ctx);
        assert!(result.changes > 0);
        assert!(evaluate(&result.draft, &ctx).score < before.score);
        assert_eq!(result.draft, result.draft.clone());
    }

    #[test]
    fn suggestions_can_be_limited_to_one_team() {
        let (ctx, draft) = nine();
        let broken = draft
            .applied(&Op::Swap {
                course: id(102),
                a: id(1),
                b: id(8),
            })
            .unwrap();
        for suggestion in suggest(&broken, &ctx, Some(id(1)), 10) {
            assert!(
                suggestion.op.teams().contains(&id(1)),
                "{:?}",
                suggestion.op
            );
        }
    }

    /// Team 9 cancels, team 10 registers: the plan has holes and `improve` fills them.
    #[test]
    fn a_replacement_team_is_placed() {
        let (mut ctx, draft) = nine();
        let data = draft.to_plan_data().unwrap();
        ctx.teams.retain(|team| team.id != id(9));
        ctx.teams.push(TeamInfo {
            id: id(10),
            name: "Team 10".into(),
            persons: 2,
        });

        let (loaded, sanitized) = Draft::from_plan(&data, &ctx);
        assert!(!sanitized.is_empty());
        let diagnosis = diagnose(&loaded, &ctx, &sanitized);
        assert_eq!(diagnosis.ghosts, vec![id(9)]);
        let newcomer = diagnosis
            .affected
            .iter()
            .find(|a| a.team == id(10))
            .unwrap();
        assert_eq!(
            newcomer.reasons.len(),
            4,
            "no seat in 3 courses + no cooking role"
        );
        // The guests of the table whose host canceled are affected, too.
        assert!(diagnosis.affected.iter().any(|a| {
            a.reasons
                .iter()
                .any(|r| matches!(r, Reason::HostLeft { .. }))
        }));
        assert!(!evaluate(&loaded, &ctx).hard_ok());

        let fixed = improve(&loaded, &ctx).draft;
        let report = evaluate(&fixed, &ctx);
        assert!(report.hard_ok(), "{:?}", report.violations);
        assert!(fixed.to_plan_data().is_ok());
        assert!(diagnose(&fixed, &ctx, &Sanitized::default()).is_clean());
    }

    /// A tenth team joins a finished plan: it opens a table and takes guests from the others.
    #[test]
    fn a_new_team_opens_its_own_table() {
        let (mut ctx, draft) = nine();
        ctx.teams.push(TeamInfo {
            id: id(10),
            name: "Team 10".into(),
            persons: 2,
        });
        let before = evaluate(&draft, &ctx);
        assert!(!before.hard_ok());

        let result = improve(&draft, &ctx);
        let fixed = result.draft;
        let report = evaluate(&fixed, &ctx);
        println!("nine + 1: {:?} -> {:?}", result.before, result.after);
        assert!(report.hard_ok(), "{:?}", report.violations);
        assert_eq!(
            fixed
                .tables
                .iter()
                .filter(|t| t.host == Some(id(10)))
                .count(),
            1
        );
        assert!(fixed.to_plan_data().is_ok());
    }

    #[test]
    fn a_removed_course_is_dropped() {
        let (mut ctx, draft) = nine();
        let data = draft.to_plan_data().unwrap();
        ctx.courses
            .retain(|course: &CourseInfo| course.id != id(103));
        let (loaded, sanitized) = Draft::from_plan(&data, &ctx);
        assert_eq!(sanitized.dropped_tables, 3);
        assert_eq!(loaded.tables.len(), 6);
    }
}
