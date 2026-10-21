use std::collections::HashMap;

use uuid::Uuid;

use super::{CourseData, HostingData, PlanData, TeamData};

#[derive(Default, Debug, Clone, PartialEq)]
pub struct Hosting {
    /// Derived from course and host (see `HostingData::id`), the backend does not store hosting ids.
    pub id: Uuid,
    pub course: CourseData,
    pub host: TeamData,
    pub guest_list: Vec<TeamData>,
}

impl Hosting {
    /// `None` if the course or the host is unknown. Unknown guests are left out.
    fn from_hosting_data(
        hosting_data: &HostingData,
        courses: &HashMap<Uuid, &CourseData>,
        teams: &HashMap<Uuid, &TeamData>,
    ) -> Option<Self> {
        let course = courses.get(&hosting_data.name)?;
        let host = teams.get(&hosting_data.host)?;
        let guest_list = hosting_data
            .guest_list
            .iter()
            .filter_map(|guest| teams.get(guest).map(|team| (*team).clone()))
            .collect();

        Some(Hosting {
            id: hosting_data.id(),
            course: (*course).clone(),
            host: (*host).clone(),
            guest_list,
        })
    }
}

#[derive(Default, Debug, Clone)]
pub struct Plan {
    pub hosting_list: Vec<Hosting>,
    /// For every team that takes part in at least one hosting (as host or guest): its hostings
    /// ordered by the time of the course. Derived from `hosting_list`, nothing is stored for it.
    pub walking_path: HashMap<TeamData, Vec<Hosting>>,
}

impl Plan {
    /// Resolves the ids of the stored plan into full courses and teams.
    ///
    /// The backend does not verify that a plan only references existing courses and teams, so
    /// dangling references are skipped instead of panicking: a hosting with an unknown course or
    /// host is left out, unknown guests are removed from the guest list.
    pub fn from_plan_data(
        plan_data: &PlanData,
        course_list: &[CourseData],
        team_list: &[TeamData],
    ) -> Self {
        let courses: HashMap<Uuid, &CourseData> = course_list.iter().map(|c| (c.id, c)).collect();
        let teams: HashMap<Uuid, &TeamData> = team_list.iter().map(|t| (t.id, t)).collect();

        let hosting_list: Vec<Hosting> = plan_data
            .hosting_list
            .iter()
            .filter_map(|h| Hosting::from_hosting_data(h, &courses, &teams))
            .collect();

        let mut paths: HashMap<Uuid, Vec<Hosting>> = HashMap::new();
        for hosting in &hosting_list {
            let mut participants: Vec<Uuid> = vec![hosting.host.id];
            for guest in &hosting.guest_list {
                if !participants.contains(&guest.id) {
                    participants.push(guest.id);
                }
            }
            for team_id in participants {
                paths.entry(team_id).or_default().push(hosting.clone());
            }
        }

        let walking_path: HashMap<TeamData, Vec<Hosting>> = paths
            .into_iter()
            .filter_map(|(team_id, mut hostings)| {
                // Stable sort: hostings at the same time keep the order of the plan.
                hostings.sort_by_key(|hosting| hosting.course.time);
                teams.get(&team_id).map(|team| ((*team).clone(), hostings))
            })
            .collect();

        Plan {
            hosting_list,
            walking_path,
        }
    }
}
