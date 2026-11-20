use std::collections::HashMap;
use std::vec;

use uuid::Uuid;

use crate::calculator::{Calculator, Course, Hosting, Plan, Point, Team};

#[derive(Hash, Eq, PartialEq, Clone, Copy)]
enum CourseType {
    Appetizer,
    MainCourse,
    Dessert,
}

impl CourseType {
    fn rank(&self) -> usize {
        match self {
            CourseType::Appetizer => 0,
            CourseType::MainCourse => 1,
            CourseType::Dessert => 2,
        }
    }
}

struct AssignedCourse {
    course_type: CourseType,
    host: Uuid,
    guest_list: Vec<Uuid>,
}

struct Group<'a> {
    sorted_team_list: Vec<&'a Team>,
}

impl<'a> Group<'a> {
    fn new() -> Self {
        Self {
            sorted_team_list: Vec::new(),
        }
    }
}

struct SchemaCourse {
    course_type: CourseType,
    host: u8,
    guest_list: Vec<u8>,
}

struct Schema {
    plan: HashMap<u8 /* Host Id */, SchemaCourse>,
}

impl Schema {
    /// A usable schema lets every team host exactly once and attend every course exactly once.
    /// (The schemas for 13 and 14 teams do not fulfil this yet; using them used to panic.)
    fn is_consistent(&self, size: u8) -> bool {
        if self.plan.len() != size as usize {
            return false;
        }
        let mut attendance: HashMap<(u8, usize), u32> = HashMap::new();
        let mut hosts = std::collections::HashSet::new();
        for (key, course) in &self.plan {
            if *key != course.host || !hosts.insert(course.host) {
                return false;
            }
            let rank = course.course_type.rank();
            for team in std::iter::once(&course.host).chain(&course.guest_list) {
                if *team == 0 || *team > size {
                    return false;
                }
                *attendance.entry((*team, rank)).or_default() += 1;
            }
        }
        (1..=size).all(|team| (0..3).all(|rank| attendance.get(&(team, rank)) == Some(&1)))
    }

    fn create_mapping(&self, sorted_team_list: &Vec<&Team>) -> HashMap<u8, Uuid> {
        let mut mapping = HashMap::new();

        let mut team_idx = 0 as u8;
        let mut sorted_plan = self
            .plan
            .iter()
            .map(|(host_idx, course)| (host_idx, course.course_type.rank()))
            .collect::<Vec<(&u8, usize)>>();
        sorted_plan.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(b.0)));
        sorted_plan.iter().for_each(|(idx, _course)| {
            mapping.insert(**idx, sorted_team_list[team_idx as usize].id);
            team_idx += 1;
        });

        mapping
    }
}

pub struct SchemaCalculator<'a> {
    schema_list: HashMap<u8 /* Number of teams */, Schema>,
    target_lat: f64,
    target_lon: f64,
    team_list: &'a Vec<Team>,
    sorted_course_list: Vec<&'a Course>,
}

impl<'a> SchemaCalculator<'a> {
    pub fn new(
        end_point: &Point,
        team_list: &'a Vec<Team>,
        course_list: &'a Vec<Course>,
    ) -> Result<Self, String> {
        if team_list.len() < 9 {
            return Err("At least 9 teams are required in SchemaCalculator".to_string());
        }
        if course_list.len() != 3 {
            return Err("Exactly 3 courses are required in SchemaCalculator".to_string());
        }
        let valid = |point: &Point| point.latitude.is_finite() && point.longitude.is_finite();
        if !valid(end_point) || team_list.iter().any(|team| !valid(&team.point)) {
            return Err("The end point and all teams need valid address coordinates".to_string());
        }

        let mut sorted_course_list = course_list.iter().collect::<Vec<&Course>>();
        sorted_course_list.sort_by_key(|course| course.time);

        let calculator = SchemaCalculator {
            schema_list: HashMap::from([
                (
                    9,
                    Schema {
                        plan: HashMap::from([
                            (
                                1,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 1,
                                    guest_list: vec![2, 3],
                                },
                            ),
                            (
                                4,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 4,
                                    guest_list: vec![5, 6],
                                },
                            ),
                            (
                                7,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 7,
                                    guest_list: vec![8, 9],
                                },
                            ),
                            (
                                2,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 2,
                                    guest_list: vec![6, 7],
                                },
                            ),
                            (
                                5,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 5,
                                    guest_list: vec![9, 1],
                                },
                            ),
                            (
                                8,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 8,
                                    guest_list: vec![3, 4],
                                },
                            ),
                            (
                                3,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 3,
                                    guest_list: vec![7, 5],
                                },
                            ),
                            (
                                6,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 6,
                                    guest_list: vec![1, 8],
                                },
                            ),
                            (
                                9,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 9,
                                    guest_list: vec![4, 2],
                                },
                            ),
                        ]),
                    },
                ),
                (
                    10,
                    Schema {
                        plan: HashMap::from([
                            (
                                1,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 1,
                                    guest_list: vec![2, 3],
                                },
                            ),
                            (
                                4,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 4,
                                    guest_list: vec![5, 6],
                                },
                            ),
                            (
                                7,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 7,
                                    guest_list: vec![8, 9, 10],
                                },
                            ),
                            (
                                2,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 2,
                                    guest_list: vec![6, 7],
                                },
                            ),
                            (
                                5,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 5,
                                    guest_list: vec![9, 1],
                                },
                            ),
                            (
                                8,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 8,
                                    guest_list: vec![3],
                                },
                            ),
                            (
                                10,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 10,
                                    guest_list: vec![4],
                                },
                            ),
                            (
                                3,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 3,
                                    guest_list: vec![7, 5, 10],
                                },
                            ),
                            (
                                6,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 6,
                                    guest_list: vec![1, 8],
                                },
                            ),
                            (
                                9,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 9,
                                    guest_list: vec![4, 2],
                                },
                            ),
                        ]),
                    },
                ),
                (
                    11,
                    Schema {
                        plan: HashMap::from([
                            (
                                1,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 1,
                                    guest_list: vec![2, 3],
                                },
                            ),
                            (
                                4,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 4,
                                    guest_list: vec![5, 6],
                                },
                            ),
                            (
                                7,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 7,
                                    guest_list: vec![8, 9],
                                },
                            ),
                            (
                                10,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 10,
                                    guest_list: vec![11],
                                },
                            ),
                            (
                                2,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 2,
                                    guest_list: vec![6, 7],
                                },
                            ),
                            (
                                5,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 5,
                                    guest_list: vec![9, 1],
                                },
                            ),
                            (
                                8,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 8,
                                    guest_list: vec![3, 10],
                                },
                            ),
                            (
                                11,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 11,
                                    guest_list: vec![4],
                                },
                            ),
                            (
                                3,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 3,
                                    guest_list: vec![7, 5, 11],
                                },
                            ),
                            (
                                6,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 6,
                                    guest_list: vec![1, 8],
                                },
                            ),
                            (
                                9,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 9,
                                    guest_list: vec![4, 2, 10],
                                },
                            ),
                        ]),
                    },
                ),
                (
                    12,
                    Schema {
                        plan: HashMap::from([
                            (
                                1,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 1,
                                    guest_list: vec![2, 3],
                                },
                            ),
                            (
                                4,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 4,
                                    guest_list: vec![5, 6],
                                },
                            ),
                            (
                                7,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 7,
                                    guest_list: vec![8, 9],
                                },
                            ),
                            (
                                10,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 10,
                                    guest_list: vec![11, 12],
                                },
                            ),
                            (
                                2,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 2,
                                    guest_list: vec![6, 10],
                                },
                            ),
                            (
                                5,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 5,
                                    guest_list: vec![9, 1],
                                },
                            ),
                            (
                                8,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 8,
                                    guest_list: vec![12, 4],
                                },
                            ),
                            (
                                11,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 11,
                                    guest_list: vec![3, 7],
                                },
                            ),
                            (
                                3,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 3,
                                    guest_list: vec![10, 5],
                                },
                            ),
                            (
                                6,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 6,
                                    guest_list: vec![1, 8],
                                },
                            ),
                            (
                                9,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 9,
                                    guest_list: vec![4, 11],
                                },
                            ),
                            (
                                12,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 12,
                                    guest_list: vec![7, 2],
                                },
                            ),
                        ]),
                    },
                ),
                (
                    13,
                    Schema {
                        plan: HashMap::from([
                            (
                                1,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 1,
                                    guest_list: vec![2, 3],
                                },
                            ),
                            (
                                4,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 4,
                                    guest_list: vec![5, 6],
                                },
                            ),
                            (
                                7,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 7,
                                    guest_list: vec![8, 9],
                                },
                            ),
                            (
                                10,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 10,
                                    guest_list: vec![11, 12, 13],
                                },
                            ),
                            (
                                2,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 2,
                                    guest_list: vec![6, 10, 13],
                                },
                            ),
                            (
                                5,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 5,
                                    guest_list: vec![9, 1],
                                },
                            ),
                            (
                                8,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 8,
                                    guest_list: vec![12, 4],
                                },
                            ),
                            (
                                11,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 11,
                                    guest_list: vec![3, 7],
                                },
                            ),
                            (
                                3,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 3,
                                    guest_list: vec![10, 5],
                                },
                            ),
                            (
                                6,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 6,
                                    guest_list: vec![1, 8],
                                },
                            ),
                            (
                                9,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 9,
                                    guest_list: vec![4, 11],
                                },
                            ),
                            (
                                12,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 12,
                                    guest_list: vec![7],
                                },
                            ),
                            (
                                13,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 13,
                                    guest_list: vec![2],
                                },
                            ),
                        ]),
                    },
                ),
                (
                    14,
                    Schema {
                        plan: HashMap::from([
                            (
                                1,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 1,
                                    guest_list: vec![2, 3],
                                },
                            ),
                            (
                                4,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 4,
                                    guest_list: vec![5, 6],
                                },
                            ),
                            (
                                7,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 7,
                                    guest_list: vec![8, 9],
                                },
                            ),
                            (
                                10,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 10,
                                    guest_list: vec![11, 12],
                                },
                            ),
                            (
                                13,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 13,
                                    guest_list: vec![14],
                                },
                            ),
                            (
                                2,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 2,
                                    guest_list: vec![6, 10],
                                },
                            ),
                            (
                                5,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 5,
                                    guest_list: vec![9, 1],
                                },
                            ),
                            (
                                8,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 8,
                                    guest_list: vec![12, 4],
                                },
                            ),
                            (
                                11,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 11,
                                    guest_list: vec![3, 13],
                                },
                            ),
                            (
                                14,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 14,
                                    guest_list: vec![7],
                                },
                            ),
                            (
                                3,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 3,
                                    guest_list: vec![10, 5, 14],
                                },
                            ),
                            (
                                6,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 6,
                                    guest_list: vec![1, 8, 13],
                                },
                            ),
                            (
                                9,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 9,
                                    guest_list: vec![4, 11],
                                },
                            ),
                            (
                                12,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 12,
                                    guest_list: vec![7, 2],
                                },
                            ),
                        ]),
                    },
                ),
                (
                    15,
                    Schema {
                        plan: HashMap::from([
                            (
                                1,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 1,
                                    guest_list: vec![2, 3],
                                },
                            ),
                            (
                                4,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 4,
                                    guest_list: vec![5, 6],
                                },
                            ),
                            (
                                7,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 7,
                                    guest_list: vec![8, 9],
                                },
                            ),
                            (
                                10,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 10,
                                    guest_list: vec![11, 12],
                                },
                            ),
                            (
                                13,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 13,
                                    guest_list: vec![14, 15],
                                },
                            ),
                            (
                                2,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 2,
                                    guest_list: vec![6, 13],
                                },
                            ),
                            (
                                5,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 5,
                                    guest_list: vec![9, 1],
                                },
                            ),
                            (
                                8,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 8,
                                    guest_list: vec![12, 4],
                                },
                            ),
                            (
                                11,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 11,
                                    guest_list: vec![15, 7],
                                },
                            ),
                            (
                                14,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 14,
                                    guest_list: vec![3, 10],
                                },
                            ),
                            (
                                3,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 3,
                                    guest_list: vec![13, 5],
                                },
                            ),
                            (
                                6,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 6,
                                    guest_list: vec![1, 8],
                                },
                            ),
                            (
                                9,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 9,
                                    guest_list: vec![4, 11],
                                },
                            ),
                            (
                                12,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 12,
                                    guest_list: vec![7, 14],
                                },
                            ),
                            (
                                15,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 15,
                                    guest_list: vec![10, 2],
                                },
                            ),
                        ]),
                    },
                ),
                (
                    16,
                    Schema {
                        plan: HashMap::from([
                            (
                                1,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 1,
                                    guest_list: vec![2, 3],
                                },
                            ),
                            (
                                4,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 4,
                                    guest_list: vec![5, 6],
                                },
                            ),
                            (
                                7,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 7,
                                    guest_list: vec![8, 9],
                                },
                            ),
                            (
                                10,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 10,
                                    guest_list: vec![11, 12],
                                },
                            ),
                            (
                                13,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 13,
                                    guest_list: vec![14, 15, 16],
                                },
                            ),
                            (
                                2,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 2,
                                    guest_list: vec![6, 13],
                                },
                            ),
                            (
                                5,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 5,
                                    guest_list: vec![9, 1],
                                },
                            ),
                            (
                                8,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 8,
                                    guest_list: vec![12, 4],
                                },
                            ),
                            (
                                11,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 11,
                                    guest_list: vec![15, 7],
                                },
                            ),
                            (
                                14,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 14,
                                    guest_list: vec![3],
                                },
                            ),
                            (
                                16,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 16,
                                    guest_list: vec![10],
                                },
                            ),
                            (
                                3,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 3,
                                    guest_list: vec![13, 5],
                                },
                            ),
                            (
                                6,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 6,
                                    guest_list: vec![1, 8, 16],
                                },
                            ),
                            (
                                9,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 9,
                                    guest_list: vec![4, 11],
                                },
                            ),
                            (
                                12,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 12,
                                    guest_list: vec![7, 14],
                                },
                            ),
                            (
                                15,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 15,
                                    guest_list: vec![10, 2],
                                },
                            ),
                        ]),
                    },
                ),
                (
                    17,
                    Schema {
                        plan: HashMap::from([
                            (
                                1,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 1,
                                    guest_list: vec![2, 3],
                                },
                            ),
                            (
                                4,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 4,
                                    guest_list: vec![5, 6],
                                },
                            ),
                            (
                                7,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 7,
                                    guest_list: vec![8, 9],
                                },
                            ),
                            (
                                10,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 10,
                                    guest_list: vec![11, 12],
                                },
                            ),
                            (
                                13,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 13,
                                    guest_list: vec![14, 15],
                                },
                            ),
                            (
                                16,
                                SchemaCourse {
                                    course_type: CourseType::Appetizer,
                                    host: 16,
                                    guest_list: vec![17],
                                },
                            ),
                            (
                                2,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 2,
                                    guest_list: vec![6, 13],
                                },
                            ),
                            (
                                5,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 5,
                                    guest_list: vec![9, 1],
                                },
                            ),
                            (
                                8,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 8,
                                    guest_list: vec![12, 4],
                                },
                            ),
                            (
                                11,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 11,
                                    guest_list: vec![15, 7],
                                },
                            ),
                            (
                                14,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 14,
                                    guest_list: vec![3, 16],
                                },
                            ),
                            (
                                17,
                                SchemaCourse {
                                    course_type: CourseType::MainCourse,
                                    host: 17,
                                    guest_list: vec![10],
                                },
                            ),
                            (
                                3,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 3,
                                    guest_list: vec![13, 5],
                                },
                            ),
                            (
                                6,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 6,
                                    guest_list: vec![1, 8, 17],
                                },
                            ),
                            (
                                9,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 9,
                                    guest_list: vec![4, 11, 16],
                                },
                            ),
                            (
                                12,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 12,
                                    guest_list: vec![7, 14],
                                },
                            ),
                            (
                                15,
                                SchemaCourse {
                                    course_type: CourseType::Dessert,
                                    host: 15,
                                    guest_list: vec![10, 2],
                                },
                            ),
                        ]),
                    },
                ),
            ]),
            target_lat: end_point.latitude,
            target_lon: end_point.longitude,
            team_list,
            sorted_course_list,
        };
        calculator.check_supported()?;
        Ok(calculator)
    }

    /// Teams are split into groups of 9 (the last group takes the remainder, so 9–17 teams).
    /// Every group needs a consistent schema.
    fn check_supported(&self) -> Result<(), String> {
        let team_count = self.team_list.len();
        let groups = team_count / 9;
        for index in 0..groups {
            let size = if index == groups - 1 {
                9 + team_count % 9
            } else {
                9
            } as u8;
            let supported = self
                .schema_list
                .get(&size)
                .is_some_and(|schema| schema.is_consistent(size));
            if !supported {
                return Err(format!(
                    "With {team_count} teams one group would have {size} teams, and the plan schema \
                     for that group size is not available yet. Please add or remove teams."
                ));
            }
        }
        Ok(())
    }
}

impl<'a> Calculator for SchemaCalculator<'a> {
    fn calculate(&self) -> Plan {
        let group_list = self.assign_groups();
        let course_list = self.assign_courses(&group_list);

        self.map_to_plan(course_list)
    }
}

impl<'a> SchemaCalculator<'a> {
    fn map_to_plan(&self, assigned_course_list: Vec<AssignedCourse>) -> Plan {
        let course_mapping = HashMap::from([
            (CourseType::Appetizer, self.sorted_course_list[0].id),
            (CourseType::MainCourse, self.sorted_course_list[1].id),
            (CourseType::Dessert, self.sorted_course_list[2].id),
        ]);

        // Keeps the order of the assigned courses (a HashMap made it random).
        let hosting_list = assigned_course_list
            .into_iter()
            .map(|assigned| Hosting {
                course: course_mapping[&assigned.course_type],
                host: assigned.host,
                guest_list: assigned.guest_list,
            })
            .collect();

        Plan { hosting_list }
    }

    fn calculate_angle(&self, team: &Team) -> f64 {
        let delta_lon = team.point.longitude - self.target_lon;
        let delta_lat = team.point.latitude - self.target_lat;
        delta_lat.atan2(delta_lon)
    }

    fn calculate_distance(&self, point: &Point) -> f64 {
        let delta_lon = point.longitude - self.target_lon;
        let delta_lat = point.latitude - self.target_lat;
        (delta_lon.powi(2) + delta_lat.powi(2)).sqrt()
    }

    fn assign_groups(&self) -> Vec<Group<'a>> {
        let mut teams_with_angles: Vec<(&Team, f64)> = self
            .team_list
            .into_iter()
            .map(|team| {
                let angle = self.calculate_angle(&team);
                (team, angle)
            })
            .collect();

        teams_with_angles.sort_by(|a, b| a.1.total_cmp(&b.1));

        let mut group_list = vec![];

        for i in 0..(teams_with_angles.len() / 9) {
            let is_last_group = i == teams_with_angles.len() / 9 - 1;

            let mut group = Group::new();

            let num_teams_in_group = if is_last_group {
                9 + (teams_with_angles.len() % 9)
            } else {
                9
            };

            //Add teams between i*9 and i*9+num_teams_in_group to group.team_list
            let mut team_list = teams_with_angles[i * 9..i * 9 + num_teams_in_group]
                .iter()
                .map(|(team, _angle)| (*team, self.calculate_distance(&team.point)))
                .collect::<Vec<(&Team, f64)>>();

            team_list.sort_by(|a, b| b.1.total_cmp(&a.1));

            group
                .sorted_team_list
                .extend_from_slice(&team_list.iter().map(|t| t.0).collect::<Vec<&Team>>());

            group_list.push(group);
        }

        group_list
    }

    fn assign_courses(&self, groups: &Vec<Group<'a>>) -> Vec<AssignedCourse> {
        let mut assigned_course_list = vec![];

        for group in groups {
            let schema = self
                .schema_list
                .get(&(group.sorted_team_list.len() as u8))
                .expect("No schema found for group size");

            let mapping = schema.create_mapping(&group.sorted_team_list);
            schema.plan.values().for_each(|schema_course| {
                let host = mapping
                    .get(&schema_course.host)
                    .expect("Host index not found in mapping")
                    .clone();
                let guest_list = schema_course
                    .guest_list
                    .iter()
                    .map(|guest_idx| {
                        mapping
                            .get(guest_idx)
                            .expect("Guest index not found in mapping")
                            .clone()
                    })
                    .collect();
                let assigned_course = AssignedCourse {
                    course_type: schema_course.course_type,
                    host,
                    guest_list,
                };
                assigned_course_list.push(assigned_course);
            });
        }

        assigned_course_list.sort_by(|a, b| {
            a.course_type
                .rank()
                .cmp(&b.course_type.rank())
                .then(a.host.cmp(&b.host))
        });

        assigned_course_list
    }
}
