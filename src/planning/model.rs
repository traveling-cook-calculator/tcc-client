//! The editable plan and the operations on it.

use std::collections::{HashMap, HashSet};

use chrono::NaiveTime;
use uuid::Uuid;

use crate::storage::{HostingData, MAX_GUESTS_PER_HOSTING, MIN_GUESTS_PER_HOSTING, PlanData};

/// Persons of a team that did not say how many it has.
pub const DEFAULT_PERSONS: u16 = 2;

#[derive(Clone, Debug, PartialEq)]
pub struct TeamInfo {
    pub id: Uuid,
    pub name: String,
    pub persons: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CourseInfo {
    pub id: Uuid,
    pub name: String,
    pub time: NaiveTime,
}

/// What the rules need to know besides the plan: the teams that take part and the courses.
/// Canceled teams must **not** be part of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    pub teams: Vec<TeamInfo>,
    /// Ordered by time.
    pub courses: Vec<CourseInfo>,
}

impl Context {
    pub fn new(teams: Vec<TeamInfo>, mut courses: Vec<CourseInfo>) -> Self {
        courses.sort_by_key(|course| course.time);
        Self { teams, courses }
    }

    pub fn team(&self, id: Uuid) -> Option<&TeamInfo> {
        self.teams.iter().find(|team| team.id == id)
    }

    pub fn course(&self, id: Uuid) -> Option<&CourseInfo> {
        self.courses.iter().find(|course| course.id == id)
    }

    pub fn persons(&self, id: Uuid) -> u32 {
        self.team(id)
            .map_or(DEFAULT_PERSONS, |team| team.persons.max(1)) as u32
    }

    pub fn knows_team(&self, id: Uuid) -> bool {
        self.teams.iter().any(|team| team.id == id)
    }
}

/// One hosting that can be edited. Never removed from `Draft::tables` while editing, so that the
/// index of a table stays valid for the UI.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub course: Uuid,
    /// `None`: the host left (canceled, deleted) or was moved away; somebody has to take over.
    pub host: Option<Uuid>,
    pub guests: Vec<Uuid>,
    /// Who hosted this table before it became vacant. Only a hint for the UI.
    pub former_host: Option<Uuid>,
}

impl Table {
    pub fn members(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.host.into_iter().chain(self.guests.iter().copied())
    }

    pub fn contains(&self, team: Uuid) -> bool {
        self.host == Some(team) || self.guests.contains(&team)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seat {
    Host(usize),
    Guest(usize),
}

impl Seat {
    pub fn table(self) -> usize {
        match self {
            Seat::Host(table) | Seat::Guest(table) => table,
        }
    }
}

/// Everything that was thrown out while the stored plan was loaded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sanitized {
    /// `(course, team)`: the host of a hosting is canceled, deleted or unknown.
    pub dropped_hosts: Vec<(Uuid, Uuid)>,
    /// `(course, team)`: a guest is canceled, deleted or unknown.
    pub dropped_guests: Vec<(Uuid, Uuid)>,
    /// Hostings of a course that does not exist any more.
    pub dropped_tables: usize,
    /// Indices (in `Draft::tables`) of tables that lost a guest.
    pub changed_tables: Vec<usize>,
}

impl Sanitized {
    pub fn is_empty(&self) -> bool {
        self.dropped_hosts.is_empty() && self.dropped_guests.is_empty() && self.dropped_tables == 0
    }

    /// Teams that are still referenced by the stored plan but do not take part any more.
    pub fn ghosts(&self) -> Vec<Uuid> {
        let mut ghosts: Vec<Uuid> = Vec::new();
        for (_, team) in self.dropped_hosts.iter().chain(&self.dropped_guests) {
            if !ghosts.contains(team) {
                ghosts.push(*team);
            }
        }
        ghosts
    }
}

/// A change of the plan. Everything the UI offers (drag and drop, selects, suggestions) is one of
/// these, so that the same checks and the same undo apply to all of it.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// `team` goes to `to_table` as a guest (it leaves its old seat in that course).
    Move {
        course: Uuid,
        team: Uuid,
        to_table: usize,
    },
    /// `a` and `b` exchange their seats in one course. A team without seat can take over a seat.
    Swap { course: Uuid, a: Uuid, b: Uuid },
    /// `a` and `b` exchange their seats in every course in which one of them is the host. This
    /// swaps who cooks which course, and keeps both teams consistent in all courses.
    SwapRoles { a: Uuid, b: Uuid },
    /// `team` becomes the host of a table without host.
    SeatHost { table: usize, team: Uuid },
    /// A new table of `course`, hosted by `host`, with guests taken from their old seats.
    OpenTable {
        course: Uuid,
        host: Uuid,
        take: Vec<Uuid>,
    },
    /// `team` leaves its seat in `course`.
    Unseat { course: Uuid, team: Uuid },
}

impl Op {
    /// Teams whose seat changes.
    pub fn teams(&self) -> Vec<Uuid> {
        match self {
            Op::Move { team, .. } | Op::SeatHost { team, .. } | Op::Unseat { team, .. } => {
                vec![*team]
            }
            Op::Swap { a, b, .. } | Op::SwapRoles { a, b } => vec![*a, *b],
            Op::OpenTable { host, take, .. } => {
                let mut teams = vec![*host];
                teams.extend(take.iter().copied());
                teams
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    pub tables: Vec<Table>,
}

impl Draft {
    /// Builds the editable plan from the stored one. Teams that do not take part any more are
    /// removed (their seat becomes vacant); what was removed is reported.
    pub fn from_plan(plan: &PlanData, ctx: &Context) -> (Draft, Sanitized) {
        let mut sanitized = Sanitized::default();
        let mut tables = Vec::new();

        for hosting in &plan.hosting_list {
            if ctx.course(hosting.name).is_none() {
                sanitized.dropped_tables += 1;
                continue;
            }
            let host = if ctx.knows_team(hosting.host) {
                Some(hosting.host)
            } else {
                sanitized.dropped_hosts.push((hosting.name, hosting.host));
                None
            };
            let mut guests: Vec<Uuid> = Vec::new();
            for guest in &hosting.guest_list {
                if !ctx.knows_team(*guest) {
                    sanitized.dropped_guests.push((hosting.name, *guest));
                    if sanitized.changed_tables.last() != Some(&tables.len()) {
                        sanitized.changed_tables.push(tables.len());
                    }
                } else if !guests.contains(guest) && host != Some(*guest) {
                    guests.push(*guest);
                }
            }
            tables.push(Table {
                course: hosting.name,
                host,
                guests,
                former_host: if host.is_none() {
                    Some(hosting.host)
                } else {
                    None
                },
            });
        }
        (Draft { tables }, sanitized)
    }

    /// The plan as it is stored. Tables without host and guests are dropped; anything else the
    /// storage would refuse is an error.
    pub fn to_plan_data(&self) -> Result<PlanData, String> {
        let mut hosting_list = Vec::new();
        for table in &self.tables {
            if table.host.is_none() && table.guests.is_empty() {
                continue;
            }
            let Some(host) = table.host else {
                return Err("A table has no host.".to_string());
            };
            let guests = table.guests.len();
            if !(MIN_GUESTS_PER_HOSTING..=MAX_GUESTS_PER_HOSTING).contains(&guests) {
                return Err(format!(
                    "A table must have between {MIN_GUESTS_PER_HOSTING} and {MAX_GUESTS_PER_HOSTING} guests (found {guests})."
                ));
            }
            hosting_list.push(HostingData {
                name: table.course,
                host,
                guest_list: table.guests.clone(),
            });
        }
        Ok(PlanData {
            hosting_list,
            stale_since: None,
        })
    }

    /// Independent of the order of tables and guests: is this the same plan?
    pub fn same_plan(&self, other: &Draft) -> bool {
        self.canonical() == other.canonical()
    }

    fn canonical(&self) -> Vec<(Uuid, Option<Uuid>, Vec<Uuid>)> {
        let mut tables: Vec<_> = self
            .tables
            .iter()
            .filter(|table| table.host.is_some() || !table.guests.is_empty())
            .map(|table| {
                let mut guests = table.guests.clone();
                guests.sort();
                (table.course, table.host, guests)
            })
            .collect();
        tables.sort();
        tables
    }

    // ----- lookups ------------------------------------------------------------------------

    pub fn seats(&self, course: Uuid, team: Uuid) -> Vec<Seat> {
        let mut seats = Vec::new();
        for (index, table) in self.tables.iter().enumerate() {
            if table.course != course {
                continue;
            }
            if table.host == Some(team) {
                seats.push(Seat::Host(index));
            }
            if table.guests.contains(&team) {
                seats.push(Seat::Guest(index));
            }
        }
        seats
    }

    pub fn seat(&self, course: Uuid, team: Uuid) -> Option<Seat> {
        self.seats(course, team).into_iter().next()
    }

    /// The course `team` cooks (the first one, if it wrongly cooks several).
    pub fn cooking_course(&self, team: Uuid) -> Option<Uuid> {
        self.tables
            .iter()
            .find(|table| table.host == Some(team))
            .map(|table| table.course)
    }

    pub fn tables_of(&self, course: Uuid) -> impl Iterator<Item = (usize, &Table)> {
        self.tables
            .iter()
            .enumerate()
            .filter(move |(_, table)| table.course == course)
    }

    /// Teams of `ctx` that have no seat in `course`.
    pub fn unseated(&self, course: Uuid, ctx: &Context) -> Vec<Uuid> {
        let seated: HashSet<Uuid> = self
            .tables_of(course)
            .flat_map(|(_, table)| table.members())
            .collect();
        ctx.teams
            .iter()
            .map(|team| team.id)
            .filter(|id| !seated.contains(id))
            .collect()
    }

    /// The table `team` visits in `course` and who hosts it.
    pub fn host_of(&self, course: Uuid, team: Uuid) -> Option<Option<Uuid>> {
        self.seat(course, team)
            .map(|seat| self.tables[seat.table()].host)
    }

    // ----- changes ------------------------------------------------------------------------

    /// Removes `team` from all seats in `course`. Returns where it sat.
    fn take_out(&mut self, course: Uuid, team: Uuid) -> Option<Seat> {
        let first = self.seat(course, team);
        for table in self
            .tables
            .iter_mut()
            .filter(|table| table.course == course)
        {
            if table.host == Some(team) {
                table.host = None;
                table.former_host = Some(team);
            }
            table.guests.retain(|guest| *guest != team);
        }
        first
    }

    fn put(&mut self, seat: Seat, team: Uuid) {
        match seat {
            Seat::Host(index) => {
                self.tables[index].host = Some(team);
                self.tables[index].former_host = None;
            }
            Seat::Guest(index) => {
                if !self.tables[index].guests.contains(&team) {
                    self.tables[index].guests.push(team);
                }
            }
        }
    }

    /// `Ok` with the new plan, or why the change is not possible. The plan itself is not touched.
    pub fn applied(&self, op: &Op) -> Result<Draft, String> {
        let mut next = self.clone();
        next.apply(op)?;
        Ok(next)
    }

    pub fn apply(&mut self, op: &Op) -> Result<(), String> {
        match op {
            Op::Move {
                course,
                team,
                to_table,
            } => {
                let table = self
                    .tables
                    .get(*to_table)
                    .ok_or("This table does not exist.")?;
                if table.course != *course {
                    return Err("A team can only move within the same course.".to_string());
                }
                if table.host == Some(*team) {
                    return Ok(());
                }
                if !table.guests.contains(team) && table.guests.len() >= MAX_GUESTS_PER_HOSTING {
                    return Err(format!(
                        "A table takes at most {MAX_GUESTS_PER_HOSTING} guests."
                    ));
                }
                self.take_out(*course, *team);
                self.put(Seat::Guest(*to_table), *team);
            }
            Op::Swap { course, a, b } => {
                if a == b {
                    return Ok(());
                }
                let (seat_a, seat_b) = (self.seat(*course, *a), self.seat(*course, *b));
                if seat_a.is_none() && seat_b.is_none() {
                    return Err("Neither team has a seat in this course.".to_string());
                }
                // Both are taken out first: `put` must not see the other team's old seat.
                self.take_out(*course, *a);
                self.take_out(*course, *b);
                if let Some(seat) = seat_a {
                    self.put(seat, *b);
                }
                if let Some(seat) = seat_b {
                    self.put(seat, *a);
                }
            }
            Op::SwapRoles { a, b } => {
                if a == b {
                    return Ok(());
                }
                let courses: Vec<Uuid> = self
                    .tables
                    .iter()
                    .filter(|table| table.host == Some(*a) || table.host == Some(*b))
                    .map(|table| table.course)
                    .collect();
                if courses.is_empty() {
                    return Err("Neither team cooks.".to_string());
                }
                let mut done: Vec<Uuid> = Vec::new();
                for course in courses {
                    if done.contains(&course) {
                        continue;
                    }
                    done.push(course);
                    self.apply(&Op::Swap {
                        course,
                        a: *a,
                        b: *b,
                    })?;
                }
            }
            Op::SeatHost { table, team } => {
                let course = self
                    .tables
                    .get(*table)
                    .ok_or("This table does not exist.")?
                    .course;
                if self.tables[*table].host.is_some() {
                    return Err("This table already has a host.".to_string());
                }
                self.take_out(course, *team);
                self.put(Seat::Host(*table), *team);
            }
            Op::OpenTable { course, host, take } => {
                if take.len() > MAX_GUESTS_PER_HOSTING {
                    return Err(format!(
                        "A table takes at most {MAX_GUESTS_PER_HOSTING} guests."
                    ));
                }
                self.take_out(*course, *host);
                for guest in take {
                    self.take_out(*course, *guest);
                }
                self.tables.push(Table {
                    course: *course,
                    host: Some(*host),
                    guests: take.iter().copied().filter(|guest| guest != host).collect(),
                    former_host: None,
                });
            }
            Op::Unseat { course, team } => {
                self.take_out(*course, *team);
            }
        }
        Ok(())
    }

    /// Index of every team of the plan, for the fast rule check.
    pub(crate) fn index_of(ctx: &Context) -> HashMap<Uuid, usize> {
        ctx.teams
            .iter()
            .enumerate()
            .map(|(index, team)| (team.id, index))
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    /// 9 teams (ids 1..=9, 2 persons each), 3 courses (ids 101..=103) planned like the calculator
    /// does it: nobody meets twice.
    pub fn nine() -> (Context, Draft) {
        let teams = (1..=9)
            .map(|n| TeamInfo {
                id: id(n),
                name: format!("Team {n}"),
                persons: 2,
            })
            .collect();
        let courses = (0..3)
            .map(|n| CourseInfo {
                id: id(101 + n),
                name: format!("Course {n}"),
                time: NaiveTime::from_hms_opt(18 + n as u32, 0, 0).unwrap(),
            })
            .collect();
        let ctx = Context::new(teams, courses);
        // Teams on a 3x3 grid: course 1 = rows, course 2 = columns, course 3 = diagonals.
        // The first team of a group is the host; every team cooks exactly once.
        let layout: [[[u128; 3]; 3]; 3] = [
            [[1, 2, 3], [4, 5, 6], [8, 7, 9]],
            [[7, 1, 4], [2, 5, 8], [9, 3, 6]],
            [[5, 1, 9], [6, 2, 7], [3, 4, 8]],
        ];
        let mut tables = Vec::new();
        for (course, groups) in layout.iter().enumerate() {
            for group in groups {
                tables.push(Table {
                    course: id(101 + course as u128),
                    host: Some(id(group[0])),
                    guests: vec![id(group[1]), id(group[2])],
                    former_host: None,
                });
            }
        }
        (ctx, Draft { tables })
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    #[test]
    fn plan_data_round_trip() {
        let (ctx, draft) = nine();
        let data = draft.to_plan_data().unwrap();
        let (again, sanitized) = Draft::from_plan(&data, &ctx);
        assert!(sanitized.is_empty());
        assert_eq!(draft, again);
    }

    #[test]
    fn unknown_teams_are_removed_and_reported() {
        let (ctx, draft) = nine();
        let mut data = draft.to_plan_data().unwrap();
        data.hosting_list[0].host = id(99);
        data.hosting_list[1].guest_list.push(id(98));
        let (loaded, sanitized) = Draft::from_plan(&data, &ctx);
        assert_eq!(loaded.tables[0].host, None);
        assert_eq!(loaded.tables[0].former_host, Some(id(99)));
        assert_eq!(sanitized.dropped_hosts, vec![(id(101), id(99))]);
        assert_eq!(sanitized.dropped_guests, vec![(id(101), id(98))]);
        assert_eq!(sanitized.ghosts().len(), 2);
        assert!(
            loaded.to_plan_data().is_err(),
            "a table without host cannot be stored"
        );
    }

    #[test]
    fn move_keeps_one_seat_per_course() {
        let (_, draft) = nine();
        let moved = draft
            .applied(&Op::Move {
                course: id(101),
                team: id(2),
                to_table: 1,
            })
            .unwrap();
        assert_eq!(moved.seats(id(101), id(2)), vec![Seat::Guest(1)]);
        assert_eq!(moved.tables[0].guests, vec![id(3)]);
        assert_eq!(moved.tables[1].guests.len(), 3);
    }

    #[test]
    fn move_into_a_full_table_is_refused() {
        let (_, mut draft) = nine();
        draft.tables[1].guests = vec![id(10), id(11), id(12), id(13), id(14)];
        let result = draft.applied(&Op::Move {
            course: id(101),
            team: id(2),
            to_table: 1,
        });
        assert!(result.is_err());
    }

    #[test]
    fn swap_exchanges_seats_including_the_host_seat() {
        let (_, draft) = nine();
        let swapped = draft
            .applied(&Op::Swap {
                course: id(101),
                a: id(1),
                b: id(5),
            })
            .unwrap();
        assert_eq!(swapped.tables[0].host, Some(id(5)));
        assert_eq!(swapped.tables[1].guests, vec![id(6), id(1)]);
    }

    #[test]
    fn swap_roles_keeps_everybody_cooking_once() {
        let (ctx, draft) = nine();
        // Team 1 cooks course 1, team 2 cooks course 2.
        let a = id(1);
        let b = id(2);
        let swapped = draft.applied(&Op::SwapRoles { a, b }).unwrap();
        let report = crate::planning::evaluate(&swapped, &ctx);
        assert!(
            report.violations.is_empty(),
            "swapping roles must not break a hard rule: {:?}",
            report.violations
        );
        assert_ne!(draft.cooking_course(a), swapped.cooking_course(a));
    }

    #[test]
    fn open_table_takes_guests_from_their_tables() {
        let (_, draft) = nine();
        let opened = draft
            .applied(&Op::OpenTable {
                course: id(101),
                host: id(2),
                take: vec![id(6), id(9)],
            })
            .unwrap();
        assert_eq!(opened.tables.len(), 10);
        assert_eq!(opened.tables[9].host, Some(id(2)));
        assert_eq!(opened.tables[9].guests, vec![id(6), id(9)]);
    }

    #[test]
    fn same_plan_ignores_order() {
        let (_, draft) = nine();
        let mut shuffled = draft.clone();
        shuffled.tables.reverse();
        shuffled.tables[0].guests.reverse();
        assert!(draft.same_plan(&shuffled));
    }
}
