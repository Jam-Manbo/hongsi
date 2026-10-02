export type Profile = { name: string; hasPicture?: boolean; studentId?: string; department?: string | null };

export type ActiveLecture = { key: string; name: string; time: string; code?: string | null };
export type ActiveLectures = { items: ActiveLecture[]; message: string | null };

export type ClassSlot = {
  code: string | null;
  name: string;
  weekday: number;
  start: string;
  periods: number[];
  room: string | null;
};
export type Timetable = { slots: ClassSlot[] };

export type MarkKind = 'present' | 'late' | 'absent' | 'excused' | 'none' | 'planned' | 'other';
export type AttendanceReceipt = { lecture: ActiveLecture; date: string; kind: 'present' | 'late' | 'excused'; confirmedAt: number };
export type AttendanceSubmission = { message: string; receipt: AttendanceReceipt | null; synced: boolean };
export type AttendanceMark = { date: string; mark: string; kind: MarkKind };
export type AttendanceWeek = { week: number; sessions: AttendanceMark[] };
export type AttendanceSummary = {
  present: number;
  late: number;
  absent: number;
  excused: number;
  none: number;
  planned: number;
};
export type AttendanceCourse = {
  code: string;
  name: string;
  published: boolean;
  notice: string | null;
  weeks: AttendanceWeek[];
  summary: AttendanceSummary;
};

export type Course = { id: number; name: string; code: string | null };
export type Attachment = { name: string; size: number | null; mime: string | null };
export type ItemStatus =
  | 'submitted'
  | 'draft'
  | 'not_submitted'
  | 'overdue'
  | 'unknown'
  | 'done'
  | 'partial'
  | 'missed'
  | 'todo'
  | 'upcoming';

export type CalendarItem = {
  key: string;
  kind: 'assignment' | 'vod';
  courseId: number;
  title: string;
  start: number | null;
  due: number | null;
  status: ItemStatus;
  done: boolean;
  doneOverride: boolean | null;
  alert: boolean;
  alertLeads: number[] | null;
  url: string;
  introHtml: string | null;
  attachments: Attachment[];
  watch: { required: string | null; watched: string | null; mark: string | null } | null;
  lateUntil: number | null;
  modified: number | null;
  firstSeen: number | null;
  firstDue: number | null;
  firstIntroHtml: string | null;
  changeCount: number;
  submit: SubmitConfig | null;
};

export type SubmitConfig = {
  files: boolean;
  maxFiles: number;
  maxBytes: number;
  text: boolean;
  drafts: boolean;
  statement: boolean;
};

export type SubmissionView = {
  info: {
    status: 'submitted' | 'draft' | 'not_submitted' | 'unknown';
    canEdit: boolean;
    locked: boolean;
    files: Attachment[];
    modified: number | null;
  };
  config: SubmitConfig;
  due: number | null;
  cutoff: number | null;
  late: boolean;
  closed: boolean;
};
export type CalendarData = { courses: Course[]; items: CalendarItem[]; fetchedAt: number };

export type Meal = { name: string; start: string | null; end: string | null; price: string | null; items: string[] };
export type MealPlace = { name: string; meals: Meal[] };
export type MealDay = { date: string; weekday: string; places: MealPlace[] };

export type SeatState = 'free' | 'used' | 'blocked';
export type SeatCell = { no: number; state: SeatState };
export type Room = { no: number; name: string; total: number; used: number; free: number; grid: (SeatCell | null)[][] };
export type Building = { id: string; name: string; rooms: Room[] };
export type SeatsData = { fetchedAt: number; watching: boolean; buildings: Building[] };

export type SeatPeriod = 'semester' | 'exam' | 'vacation';
export type SeatSession = {
  id: number;
  building: string;
  buildingName: string;
  roomNo: number;
  roomName: string;
  seatNo: number;
  period: SeatPeriod;
  startedAt: number;
  startSource: 'detected' | 'manual' | 'adjusted';
  expiresAt: number;
  extendCount: number;
  endedAt: number | null;
  seatState: SeatState | null;
  validityHours: number;
};
export type RecentSeat = { seatNo: number; at: number };

export type ClassNotification = { url: string; course: string; section: string; when: string; message: string; kind: string };

export type Todo = {
  id: number;
  courseId: number | null;
  parentKey: string | null;
  title: string;
  note: string;
  dueAt: number | null;
  allDay: boolean;
  doneAt: number | null;
  notify: boolean;
  alertLeads: number[] | null;
};
export type TodoInput = Omit<Todo, 'id' | 'doneAt'>;

export type DownloadRecord = {
  id: string;
  name: string;
  course: string;
  source?: FileSource;
  cmid: number;
  index: number;
  at: number;
  path: string | null;
};

export type FileSource =
  | { kind: 'assign'; cmid: number; index: number }
  | { kind: 'module'; cmid: number; index: number }
  | { kind: 'board'; cmid: number; bwid: number; index: number };

export type ModuleContents = {
  cmid: number;
  courseId: number;
  modname: string;
  name: string;
  files: Attachment[];
  link: string | null;
};

export type BoardArticle = {
  cmid: number;
  bwid: number;
  board: string;
  title: string;
  writer: string;
  posted: number | null;
  html: string;
  attachments: Attachment[];
};
