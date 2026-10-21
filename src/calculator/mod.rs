mod schema_calculator;
pub use schema_calculator::SchemaCalculator;

use chrono::NaiveTime;
use uuid::Uuid;

use crate::storage::{AddressData, CourseData, HostingData, PlanData, TeamData};

pub trait DataMapper<T> {
    fn from_data(data: &T) -> Self;
}

/// Result of a calculation. The storage layer derives everything else (hosting ids,
/// walking paths) from the hosting list, so that is all the calculator has to deliver.
pub struct Plan {
    pub hosting_list: Vec<Hosting>,
}

impl Plan {
    pub fn to_data(&self) -> PlanData {
        PlanData {
            hosting_list: self.hosting_list.iter().map(Hosting::to_data).collect(),
            stale_since: None,
        }
    }
}

pub struct Hosting {
    pub course: Uuid, /*Course ID*/
    pub host: Uuid,   /*Team ID */
    pub guest_list: Vec<Uuid /*Team ID */>,
}

impl Hosting {
    fn to_data(&self) -> HostingData {
        HostingData {
            name: self.course,
            host: self.host,
            guest_list: self.guest_list.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Team {
    pub id: Uuid,
    pub point: Point,
}

impl DataMapper<TeamData> for Team {
    fn from_data(data: &TeamData) -> Self {
        Team {
            id: data.id,
            point: Point::from_data(&data.address),
        }
    }
}

#[derive(Debug)]
pub struct Point {
    pub latitude: f64,
    pub longitude: f64,
}

impl DataMapper<AddressData> for Point {
    fn from_data(data: &AddressData) -> Self {
        Point {
            latitude: data.latitude,
            longitude: data.longitude,
        }
    }
}

pub struct Course {
    pub id: Uuid,
    pub time: NaiveTime,
}

impl DataMapper<CourseData> for Course {
    fn from_data(data: &CourseData) -> Self {
        Course {
            id: data.id,
            time: data.time,
        }
    }
}

pub trait Calculator {
    fn calculate(&self) -> Plan;
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use chrono::NaiveTime;

    use super::*;
    use crate::storage::mapper::Plan as ResolvedPlan;

    fn team_data(n: usize) -> TeamData {
        let mut team = TeamData::default();
        team.id = Uuid::from_u128(n as u128 + 1);
        team.name = format!("Team {n}");
        team.address.address = format!("Street {n}");
        team.address.latitude = 50.0 + (n as f64 * 0.013).sin() * 0.05;
        team.address.longitude = 8.0 + (n as f64 * 0.37).cos() * 0.05;
        team
    }

    fn course_data(n: u32) -> CourseData {
        let mut course = CourseData::default();
        course.id = Uuid::from_u128(1000 + n as u128);
        course.name = format!("Course {n}");
        course.time = NaiveTime::from_hms_opt(18 + n, 0, 0).unwrap();
        course
    }

    /// Runs the calculator the same way the calculation page does.
    fn calculate(team_count: usize) -> Result<(PlanData, Vec<TeamData>, Vec<CourseData>), String> {
        let teams: Vec<TeamData> = (0..team_count).map(team_data).collect();
        let courses: Vec<CourseData> = (0..3).map(course_data).collect();
        let calc_teams: Vec<Team> = teams.iter().map(Team::from_data).collect();
        let calc_courses: Vec<Course> = courses.iter().map(Course::from_data).collect();
        let end = Point { latitude: 50.0, longitude: 8.0 };
        let calculator = SchemaCalculator::new(&end, &calc_teams, &calc_courses)?;
        Ok((calculator.calculate().to_data(), teams, courses))
    }

    /// Group sizes 13 and 14 have incomplete schemas (see `Schema::is_consistent`):
    /// they must be reported as an error. Adjust this list when the schemas are completed.
    fn is_unsupported(team_count: usize) -> bool {
        let groups = team_count / 9;
        (0..groups).any(|index| {
            let size = if index == groups - 1 { 9 + team_count % 9 } else { 9 };
            size == 13 || size == 14
        })
    }

    #[test]
    fn no_team_count_panics_and_all_plans_are_valid() {
        for team_count in 9..=60 {
            let outcome = std::panic::catch_unwind(|| calculate(team_count));
            let outcome = outcome.unwrap_or_else(|_| panic!("{team_count} teams: calculator panicked"));

            if is_unsupported(team_count) {
                assert!(outcome.is_err(), "{team_count} teams should be reported as unsupported");
                continue;
            }
            let (plan, teams, courses) =
                outcome.unwrap_or_else(|e| panic!("{team_count} teams should work: {e}"));

            plan.validate().unwrap_or_else(|e| panic!("{team_count} teams: {e}"));
            assert!(!plan.is_stale());

            let resolved = ResolvedPlan::from_plan_data(&plan, &courses, &teams);
            assert_eq!(resolved.hosting_list.len(), plan.hosting_list.len(), "{team_count}: dangling ids");
            assert_eq!(resolved.walking_path.len(), team_count, "{team_count}: team without path");

            for (team, path) in &resolved.walking_path {
                let attended: HashSet<Uuid> = path.iter().map(|stop| stop.course.id).collect();
                assert_eq!(attended.len(), 3, "{team_count}: {} misses a course", team.name);
                assert_eq!(path.len(), 3, "{team_count}: {} visits a course twice", team.name);
                let times: Vec<_> = path.iter().map(|stop| stop.course.time).collect();
                assert!(times.windows(2).all(|w| w[0] <= w[1]), "path not ordered by time");
            }

            // Every hosting id is unique (used as a key in the UI).
            let ids: HashSet<Uuid> = plan.hosting_list.iter().map(HostingData::id).collect();
            assert_eq!(ids.len(), plan.hosting_list.len(), "{team_count}: duplicate hosting ids");
        }
    }

    #[test]
    fn calculation_is_deterministic() {
        let (first, ..) = calculate(9).unwrap();
        let (second, ..) = calculate(9).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn invalid_coordinates_are_an_error_not_a_panic() {
        let mut teams: Vec<Team> = (0..9).map(|n| Team::from_data(&team_data(n))).collect();
        teams[3].point.latitude = f64::NAN;
        let courses: Vec<Course> = (0..3).map(|n| Course::from_data(&course_data(n))).collect();
        let end = Point { latitude: 50.0, longitude: 8.0 };
        assert!(SchemaCalculator::new(&end, &teams, &courses).is_err());
    }

    #[test]
    fn too_few_teams_or_wrong_course_count_is_an_error_not_a_panic() {
        assert!(calculate(8).is_err());
        let teams: Vec<Team> = (0..9).map(|n| Team::from_data(&team_data(n))).collect();
        let courses: Vec<Course> = (0..2).map(|n| Course::from_data(&course_data(n))).collect();
        assert!(SchemaCalculator::new(&Point { latitude: 0.0, longitude: 0.0 }, &teams, &courses).is_err());
    }
}
