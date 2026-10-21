use uuid::Uuid;

use crate::storage::mapper::Plan as ResolvedPlan;
use crate::storage::{CourseData, MeetingPointData, TeamData, TeamPlanData};

mod markdown;
pub mod run_schedule;

#[derive(Debug, Clone, PartialEq)]
pub struct Team {
    pub name: String,
    address: String,
    mail: Option<String>,
    phone: Option<String>,
    members: Option<u16>,
    diets: Option<String>,
}

impl Team {
    fn new(team_data: &TeamData) -> Self {
        Team {
            name: team_data.name.clone(),
            address: team_data.address.address.clone(),
            mail: team_data.mail.clone(),
            phone: team_data.phone.clone(),
            members: team_data.members,
            diets: team_data.diets.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Course {
    name: String,
    time: String,
}

impl Course {
    fn new(course_data: &CourseData) -> Self {
        Course {
            name: course_data.name.clone(),
            time: course_data.time.format("%H:%M").to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct MeetingPoint {
    name: String,
    time: String,
    address: String,
}

impl MeetingPoint {
    fn new(meeting_point_data: &MeetingPointData) -> Self {
        MeetingPoint {
            name: meeting_point_data.name.clone(),
            time: meeting_point_data.time.format("%H:%M").to_string(),
            address: meeting_point_data.address.address.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    pub host: Team,
    guest_list: Vec<Team>,
    walking_path: Vec<(Course, Team)>,
    start_point: Option<MeetingPoint>,
    end_point: Option<MeetingPoint>,
}

impl Schedule {
    pub fn default(
        has_start: bool,
        has_end: bool,
        number_of_courses: u8,
        number_of_guests: u8,
        host_course: u8,
        with_mail: bool,
        with_phone: bool,
        with_members: bool,
        with_diets: bool,
    ) -> Self {
        let host_name = "[Host Team Name]".to_string();
        let guest_name = "[Guest Team Name]".to_string();
        let address = "[Address]".to_string();
        let mail = Some("[Mail]".to_string());
        let phone = Some("[Phone]".to_string());
        let members = Some(2);
        let diets = Some("[Diets]".to_string());
        let course_name = "[Course Name]".to_string();
        let end_point_name = "[Point Name]".to_string();

        let host = Team {
            name: host_name.clone(),
            address: address.clone(),
            mail: if with_mail { mail.clone() } else { None },
            phone: if with_phone { phone.clone() } else { None },
            members: if with_members { members } else { None },
            diets: if with_diets { diets.clone() } else { None },
        };

        let guest_list = (0..number_of_guests)
            .map(|_| Team {
                name: guest_name.clone(),
                address: address.clone(),
                mail: if with_mail { mail.clone() } else { None },
                phone: if with_phone { phone.clone() } else { None },
                members: if with_members { members } else { None },
                diets: if with_diets { diets.clone() } else { None },
            })
            .collect::<Vec<Team>>();

        let walking_path = (0..number_of_courses)
            .map(|i| {
                let course = Course {
                    name: course_name.clone(),
                    time: "[HH:MM]".to_string(),
                };
                let team = if i + 1 == host_course {
                    host.clone()
                } else {
                    guest_list[0].clone()
                };
                (course, team)
            })
            .collect::<Vec<(Course, Team)>>();

        let start_point = if has_start {
            Some(MeetingPoint {
                name: "Start Point".to_string(),
                time: "[HH:MM]".to_string(),
                address: address.clone(),
            })
        } else {
            None
        };

        let end_point = if has_end {
            Some(MeetingPoint {
                name: end_point_name.clone(),
                time: "[HH:MM]".to_string(),
                address: address.clone(),
            })
        } else {
            None
        };

        Schedule {
            host,
            guest_list,
            walking_path,
            start_point,
            end_point,
        }
    }

    /// Schedule sheet of a team from the local resolved plan. `None` if the team is not part of it.
    /// A team that only visits others has no guests; the sheet then shows its walking path only.
    pub fn for_team(
        team_id: Uuid,
        plan: &ResolvedPlan,
        start_point: &Option<MeetingPointData>,
        end_point: &Option<MeetingPointData>,
    ) -> Option<Self> {
        let (team, path) = plan.walking_path.iter().find(|(team, _)| team.id == team_id)?;

        // A team that hosts several courses is shown for its earliest one.
        let guest_list = plan
            .hosting_list
            .iter()
            .filter(|hosting| hosting.host.id == team_id)
            .min_by_key(|hosting| hosting.course.time)
            .map(|hosting| hosting.guest_list.iter().map(Team::new).collect())
            .unwrap_or_default();

        Some(Schedule {
            host: Team::new(team),
            guest_list,
            walking_path: path
                .iter()
                .map(|stop| (Course::new(&stop.course), Team::new(&stop.host)))
                .collect(),
            start_point: start_point.as_ref().map(MeetingPoint::new),
            end_point: end_point.as_ref().map(MeetingPoint::new),
        })
    }

    /// Schedule sheet from the backend's personal plan of a team (cloud only). One request;
    /// building it from the project's team list would need one request per team.
    pub fn from_team_plan(
        plan: &TeamPlanData,
        start_point: &Option<MeetingPointData>,
        end_point: &Option<MeetingPointData>,
    ) -> Self {
        let mut visits: Vec<_> = plan.visitations.iter().collect();
        visits.sort_by_key(|(course, _)| course.time);

        Schedule {
            host: Team {
                name: plan.host.name.clone(),
                address: plan.host.address.address.clone(),
                mail: None,
                phone: plan.host.phone.clone(),
                members: plan.host.members,
                diets: plan.host.diets.clone(),
            },
            guest_list: plan
                .host_guest
                .iter()
                .map(|guest| Team {
                    name: guest.name.clone(),
                    address: String::new(),
                    mail: None,
                    phone: guest.phone.clone(),
                    members: guest.members,
                    diets: guest.diets.clone(),
                })
                .collect(),
            walking_path: visits
                .into_iter()
                .map(|(course, host)| {
                    (
                        Course::new(course),
                        Team {
                            name: host.name.clone(),
                            address: host.address.address.clone(),
                            mail: None,
                            phone: host.phone.clone(),
                            members: None,
                            diets: None,
                        },
                    )
                })
                .collect(),
            start_point: start_point.as_ref().map(MeetingPoint::new),
            end_point: end_point.as_ref().map(MeetingPoint::new),
        }
    }
}
