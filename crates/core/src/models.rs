use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    pub has_picture: bool,
    pub department: Option<String>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveLecture {
    pub key: String,
    pub name: String,
    pub time: String,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveLectures {
    pub items: Vec<ActiveLecture>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    Present,
    Late,
    Absent,
    Excused,
    None,
    Planned,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceReceipt {
    pub lecture: ActiveLecture,
    pub date: String,
    pub kind: MarkKind,
    pub confirmed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceSubmission {
    pub message: String,
    pub receipt: Option<AttendanceReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceMark {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    pub date: String,
    pub mark: String,
    pub kind: MarkKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceWeek {
    pub week: u32,
    pub sessions: Vec<AttendanceMark>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceSummary {
    pub present: u32,
    pub late: u32,
    pub absent: u32,
    pub excused: u32,
    pub none: u32,
    pub planned: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttendanceCourse {
    #[serde(default)]
    pub cyber: bool,
    pub code: String,
    pub name: String,
    pub published: bool,
    pub notice: Option<String>,
    pub weeks: Vec<AttendanceWeek>,
    pub summary: AttendanceSummary,
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassSlot {
    pub code: Option<String>,
    pub name: String,
    pub weekday: u32,
    pub start: String,
    pub periods: Vec<u32>,
    pub room: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timetable {
    pub slots: Vec<ClassSlot>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Course {
    pub id: i64,
    pub name: String,
    pub code: Option<String>,
    pub term: Option<AcademicTerm>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcademicTerm {
    pub year: i32,
    pub semester: i32,
}

impl AcademicTerm {
    pub fn valid(self) -> bool {
        (1900..=9999).contains(&self.year) && matches!(self.semester, 10 | 11 | 20 | 21)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SemesterDisplay {
    #[default]
    Current,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionState {
    Submitted,
    NotSubmitted,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub name: String,
    pub size: Option<i64>,
    pub mime: Option<String>,
    #[serde(skip_serializing, default)]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitConfig {
    pub files: bool,
    pub max_files: u32,
    pub max_bytes: i64,
    pub text: bool,
    pub drafts: bool,
    pub statement: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionInfo {
    pub status: SubmissionState,
    pub can_edit: bool,
    pub locked: bool,
    pub files: Vec<Attachment>,
    pub modified: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assignment {
    pub id: i64,
    pub config: SubmitConfig,
    pub cmid: i64,
    pub course_id: i64,
    pub name: String,
    pub due: Option<i64>,
    pub cutoff: Option<i64>,
    pub opens: Option<i64>,
    pub modified: i64,
    pub intro_html: String,
    pub attachments: Vec<Attachment>,
    pub submission: SubmissionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VodState {
    Done,
    Partial,
    Missed,
    Todo,
    Upcoming,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vod {
    pub cmid: Option<i64>,
    pub course_id: i64,
    pub name: String,
    pub start: i64,
    pub end: i64,
    pub late_until: Option<i64>,
    pub required: Option<String>,
    pub watched: Option<String>,
    pub mark: Option<String>,
    pub state: VodState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub url: String,
    pub course: String,
    pub section: String,
    pub when: String,
    pub message: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleContents {
    pub cmid: i64,
    pub course_id: i64,
    pub modname: String,
    pub name: String,
    pub files: Vec<Attachment>,
    pub link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardArticle {
    pub cmid: i64,
    pub bwid: i64,
    pub board: String,
    pub title: String,
    pub writer: String,
    pub posted: Option<i64>,
    pub html: String,
    pub attachments: Vec<Attachment>,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meal {
    pub name: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub price: Option<String>,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MealPlace {
    pub name: String,
    pub meals: Vec<Meal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MealDay {
    pub date: String,
    pub weekday: String,
    pub places: Vec<MealPlace>,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatState {
    Free,
    Used,
    Blocked,
}

impl SeatState {
    pub fn as_str(self) -> &'static str {
        match self {
            SeatState::Free => "free",
            SeatState::Used => "used",
            SeatState::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatCell {
    pub no: u32,
    pub state: SeatState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    pub no: u32,
    pub name: String,
    pub total: u32,
    pub used: u32,
    pub free: u32,
    pub grid: Vec<Vec<Option<SeatCell>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Building {
    pub id: String,
    pub name: String,
    pub rooms: Vec<Room>,
}
